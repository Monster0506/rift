use crate::document::runtime::{BufferKindId, DocumentHandle};
use crate::document::DocumentId;
use crate::error::RiftError;
use crate::plugin::PluginGeneration;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

pub(crate) mod jobs;

use std::any::Any;

/// Maximum number of job threads allowed to run their body concurrently.
/// Newly spawned jobs queue behind this limit instead of starting immediately.
const MAX_CONCURRENT_JOBS: usize = 8;

/// Simple blocking counting semaphore used to cap concurrent job threads.
struct Semaphore {
    state: Mutex<usize>,
    condvar: Condvar,
}

impl Semaphore {
    const fn new(permits: usize) -> Self {
        Self {
            state: Mutex::new(permits),
            condvar: Condvar::new(),
        }
    }

    /// Block until a permit is available, then take it.
    fn acquire(&self) {
        let mut permits = self.state.lock().unwrap();
        while *permits == 0 {
            permits = self.condvar.wait(permits).unwrap();
        }
        *permits -= 1;
    }

    /// Return a permit and wake one waiter.
    fn release(&self) {
        {
            let mut permits = self.state.lock().unwrap();
            *permits += 1;
        }
        self.condvar.notify_one();
    }
}

/// Sealed trait for job payloads to ensure type safety.
pub trait JobPayload: Any + Send + std::fmt::Debug + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

/// Implements `JobPayload`'s downcasting boilerplate for a concrete type.
/// Not a blanket impl: that also matches `Box<dyn JobPayload>`, silently breaking every downcast.
macro_rules! impl_job_payload {
    ($t:ty) => {
        impl $crate::job_manager::JobPayload for $t {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
                self
            }
        }
    };
}
pub(crate) use impl_job_payload;
/// Async operation domain for request epoch tracking and validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsyncOpDomain {
    SyntaxParse,
    GitGutter,
    DirectoryListing,
    GitStatus,
    GitDiff,
    GitBlame,
    GitLog,
    GitShow,
    UndoTree,
    ExplorerPreview,
    FileSave,
    FileLoad,
    Terminal,
}

/// Token establishing the freshness contract for an asynchronous operation against a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncToken {
    pub(crate) handle: DocumentHandle,
    pub(crate) expected_kind: BufferKindId,
    epoch: u64,
    pub(crate) owner: Option<PluginGeneration>,
    pub(crate) domain: AsyncOpDomain,
}

impl AsyncToken {
    /// Creates a new `AsyncToken` with an explicit operation domain.
    #[must_use]
    pub(crate) const fn with_domain(
        handle: DocumentHandle,
        expected_kind: BufferKindId,
        epoch: u64,
        domain: AsyncOpDomain,
    ) -> Self {
        Self {
            handle,
            expected_kind,
            epoch,
            owner: None,
            domain,
        }
    }

    /// Creates a new `AsyncToken` with an owner generation and explicit domain.
    #[must_use]
    const fn with_owner(
        handle: DocumentHandle,
        expected_kind: BufferKindId,
        epoch: u64,
        owner: Option<PluginGeneration>,
        domain: AsyncOpDomain,
    ) -> Self {
        Self {
            handle,
            expected_kind,
            epoch,
            owner,
            domain,
        }
    }

    /// Returns the target document ID.
    #[must_use]
    pub(crate) const fn doc_id(&self) -> DocumentId {
        self.handle.doc_id
    }
}

/// Message sent from a background job to the editor.
#[derive(Debug)]
pub enum JobMessage {
    /// Job started with ID and silent flag
    Started(usize, bool),
    /// Progress update: Job ID, percentage (0-100), status message
    Progress(usize, u32, String),
    /// Job finished successfully with ID and silent flag
    Finished(usize, bool),
    /// Job failed with error message
    Error(usize, RiftError),
    /// Job cancelled (terminal state)
    Cancelled(usize),
    /// Custom payload for job-specific results
    Custom(usize, Box<dyn JobPayload>),
    /// Custom payload with an associated `AsyncToken`
    CustomToken(usize, AsyncToken, Box<dyn JobPayload>),
    /// Terminal output data (`AsyncToken`, Data)
    TerminalOutput(AsyncToken, Vec<u8>),
    /// Terminal process exit (`AsyncToken`)
    TerminalExit(AsyncToken),
}

/// Sends a job's successful result, then its Finished message. The common
/// two-message tail of a job's `run()` once it has produced a payload.
fn send_job_result(sender: &Sender<JobMessage>, id: usize, payload: Box<dyn JobPayload>) {
    let _ = sender.send(JobMessage::Custom(id, payload));
    let _ = sender.send(JobMessage::Finished(id, true));
}

/// Sends a job's successful result carrying an `AsyncToken`, then its Finished message.
fn send_job_result_with_token(
    sender: &Sender<JobMessage>,
    id: usize,
    token: AsyncToken,
    payload: Box<dyn JobPayload>,
) {
    let _ = sender.send(JobMessage::CustomToken(id, token, payload));
    let _ = sender.send(JobMessage::Finished(id, true));
}

/// Signal used to check if a job has been cancelled.
#[derive(Debug, Clone)]
pub struct CancellationSignal {
    cancelled: Arc<AtomicBool>,
}

impl CancellationSignal {
    /// Create a signal already in the given cancelled state.
    #[must_use]
    fn new(cancelled: bool) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(cancelled)),
        }
    }

    /// Create a signal that is never cancelled, for non-job callers.
    #[must_use]
    fn new_uncancelled() -> Self {
        Self::new(false)
    }

    /// Check if the job has been cancelled.
    #[must_use]
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// State of a background job
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobState {
    Running,
    Finished,
    Failed,
    Cancelled,
}

/// Handle to a running job
struct JobHandle {
    handle: JoinHandle<()>,
    state: JobState,
    cancellation_token: Arc<AtomicBool>,
    /// Whether the job opted out of notifications
    silent: bool,
    /// Descriptive name for the job type
    name: &'static str,
}

/// Trait defining a background job.
/// Jobs must be Send + 'static to be moved into a thread.
pub trait Job: Send + std::fmt::Debug + 'static {
    /// Run the job, sending messages back to the editor over `sender`. Must not access global
    /// editor state; should check `sender.send` results and the signal, exiting promptly if cancelled.
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal);

    /// Whether this job should trigger notifications in the editor.
    fn is_silent(&self) -> bool {
        false
    }

    /// Descriptive name for this job type.
    fn name(&self) -> &'static str {
        "job"
    }
    /// Returns the `AsyncToken` associated with this job, if document-targeted.
    fn async_token(&self) -> Option<AsyncToken> {
        None
    }

    /// Target document ID if this job is document-targeted.
    fn target_document_id(&self) -> Option<DocumentId> {
        None
    }

    /// Target async operation domain if this job is document-targeted.
    fn target_domain(&self) -> Option<AsyncOpDomain> {
        None
    }
}

impl Job for Box<dyn Job> {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        (*self).run(id, sender, signal);
    }

    fn is_silent(&self) -> bool {
        (**self).is_silent()
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
    fn async_token(&self) -> Option<AsyncToken> {
        (**self).async_token()
    }
    fn target_document_id(&self) -> Option<DocumentId> {
        (**self).target_document_id()
    }
    fn target_domain(&self) -> Option<AsyncOpDomain> {
        (**self).target_domain()
    }
}

impl Job for Box<dyn Job + Send> {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        (*self).run(id, sender, signal);
    }

    fn is_silent(&self) -> bool {
        (**self).is_silent()
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
    fn async_token(&self) -> Option<AsyncToken> {
        (**self).async_token()
    }
    fn target_document_id(&self) -> Option<DocumentId> {
        (**self).target_document_id()
    }
    fn target_domain(&self) -> Option<AsyncOpDomain> {
        (**self).target_domain()
    }
}

/// Extract a readable message from a caught panic payload.
fn panic_payload_to_string(payload: &Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_string())
}

/// Manages background jobs.
pub struct JobManager {
    /// Sender to clone for new jobs
    sender: Sender<JobMessage>,
    /// Receiver for the editor to poll
    receiver: Receiver<JobMessage>,
    /// Active jobs map
    jobs: BTreeMap<usize, JobHandle>,
    /// Counter for generating job IDs
    next_job_id: usize,
    /// Caps how many job bodies may run concurrently.
    concurrency_limiter: Arc<Semaphore>,
    /// Tracks tokens for running jobs: `job_id` -> `AsyncToken`
    job_tokens: HashMap<usize, AsyncToken>,
    /// Tracks current epoch per document handle and operation domain
    current_epochs: HashMap<(DocumentHandle, AsyncOpDomain), u64>,
    /// Active document handles mapped by `DocumentId`
    active_handles: HashMap<DocumentId, (DocumentHandle, BufferKindId, Option<PluginGeneration>)>,
}

impl JobManager {
    /// Create a new `JobManager`
    #[must_use]
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            jobs: BTreeMap::new(),
            next_job_id: 1,
            concurrency_limiter: Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS)),
            job_tokens: HashMap::new(),
            current_epochs: HashMap::new(),
            active_handles: HashMap::new(),
        }
    }

    /// Spawn a new job.
    /// returns the Job ID.
    pub fn spawn<J: Job>(&mut self, job: J) -> usize {
        let id = self.next_job_id;
        self.next_job_id += 1;
        if let Some(token) = job.async_token() {
            self.job_tokens.insert(id, token);
        } else if let (Some(doc_id), Some(domain)) = (job.target_document_id(), job.target_domain())
        {
            // Only tokenize when the real handle is already known; a fabricated
            // instance-0 handle would silently discard results for recycled doc IDs.
            if let Some(&(handle, kind, owner)) = self.active_handles.get(&doc_id) {
                let entry = self.current_epochs.entry((handle, domain)).or_insert(0);
                *entry += 1;
                let epoch = *entry;
                let token = AsyncToken::with_owner(handle, kind, epoch, owner, domain);
                self.job_tokens.insert(id, token);
            }
        }
        let sender = self.sender.clone();
        let cancellation_token = Arc::new(AtomicBool::new(false));
        let signal = CancellationSignal {
            cancelled: cancellation_token.clone(),
        };
        let silent = job.is_silent();
        let name = job.name();
        let job_box = Box::new(job);
        let limiter = self.concurrency_limiter.clone();

        let handle = thread::spawn(move || {
            // Signal start
            if sender.send(JobMessage::Started(id, silent)).is_ok() {
                limiter.acquire();
                if signal.is_cancelled() {
                    let _ = sender.send(JobMessage::Cancelled(id));
                } else {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        job_box.run(id, sender.clone(), signal);
                    }));
                    if let Err(payload) = result {
                        let details = panic_payload_to_string(&payload);
                        let _ = sender.send(JobMessage::Error(
                            id,
                            format!("job panicked: {details}").into(),
                        ));
                    }
                }
                limiter.release();
            }
        });

        self.jobs.insert(
            id,
            JobHandle {
                handle,
                state: JobState::Running,
                cancellation_token,
                silent,
                name,
            },
        );

        id
    }

    // ... rest of implementation updated for enum match ...

    /// Get the receiver to poll for messages.
    /// The editor should call `receiver.try_recv()` to get messages without blocking.
    #[must_use]
    pub const fn receiver(&self) -> &Receiver<JobMessage> {
        &self.receiver
    }

    /// Update job state based on message.
    /// This should be called by the editor when it processes a message.
    pub fn update_job_state(&mut self, message: &JobMessage) {
        match message {
            JobMessage::Finished(id, _) => {
                if let Some(job) = self.jobs.get_mut(id) {
                    job.state = JobState::Finished;
                }
            }
            JobMessage::Error(id, _) => {
                if let Some(job) = self.jobs.get_mut(id) {
                    job.state = JobState::Failed;
                }
            }
            JobMessage::Cancelled(id) => {
                if let Some(job) = self.jobs.get_mut(id) {
                    job.state = JobState::Cancelled;
                }
            }
            _ => {}
        }
    }

    /// Clean up finished/failed/cancelled jobs, joining their threads to release resources.
    /// Returns a list of cleaned up IDs.
    pub fn cleanup_finished_jobs(&mut self) -> Vec<usize> {
        let mut finished_ids = Vec::new();

        // Identify jobs to cleanup
        for (id, job) in &self.jobs {
            if matches!(
                job.state,
                JobState::Finished | JobState::Failed | JobState::Cancelled
            ) && job.handle.is_finished()
            {
                finished_ids.push(*id);
            }
        }

        // Remove and join
        for id in &finished_ids {
            if let Some(job) = self.jobs.remove(id) {
                let _ = job.handle.join();
            }
            self.job_tokens.remove(id);
        }

        finished_ids
    }

    /// Cancel a specific job: sets the cancellation flag and marks the state as Cancelled.
    /// The job thread is expected to notice the flag and exit.
    pub(crate) fn cancel_job(&mut self, id: usize) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.cancellation_token.store(true, Ordering::Relaxed);
            job.state = JobState::Cancelled;
        }
    }

    /// Returns true if the given job was spawned with `is_silent() == true`.
    #[must_use]
    pub(crate) fn is_job_silent(&self, id: usize) -> bool {
        self.jobs.get(&id).is_some_and(|h| h.silent)
    }

    /// Returns the descriptive name of the given job, or `"job"` if not found.
    #[must_use]
    pub(crate) fn job_name(&self, id: usize) -> &'static str {
        self.jobs.get(&id).map_or("job", |h| h.name)
    }

    #[cfg(test)]
    /// Returns the current state of a job, if it is still tracked.
    #[must_use]
    pub(crate) fn job_state(&self, id: usize) -> Option<JobState> {
        self.jobs.get(&id).map(|h| h.state)
    }

    #[cfg(test)]
    /// Returns the number of jobs ever spawned so far (monotonic counter).
    #[must_use]
    pub(crate) const fn total_spawned(&self) -> usize {
        self.next_job_id - 1
    }

    /// Whether any spawned job's OS thread is still executing, checked via
    /// the thread handle so it's accurate even if the caller isn't draining.
    #[must_use]
    pub(crate) fn any_job_thread_alive(&self) -> bool {
        self.jobs.values().any(|h| !h.handle.is_finished())
    }
    /// Spawns a job with an explicitly registered `AsyncToken`.
    pub(crate) fn spawn_with_token<J: Job>(&mut self, job: J, token: AsyncToken) -> usize {
        let id = self.spawn(job);
        self.register_job_token(id, token);
        id
    }
    /// Register an active document handle, kind, and optional plugin owner.
    pub(crate) fn register_document_handle(
        &mut self,
        handle: DocumentHandle,
        kind: BufferKindId,
        owner: Option<PluginGeneration>,
    ) {
        self.active_handles
            .insert(handle.doc_id, (handle, kind, owner));
    }

    /// Generates a fresh `AsyncToken` for the given document handle, kind, and domain,
    /// advancing the request epoch so any older token for this domain becomes stale.
    pub(crate) fn next_token(
        &mut self,
        handle: DocumentHandle,
        expected_kind: BufferKindId,
        domain: AsyncOpDomain,
    ) -> AsyncToken {
        self.register_document_handle(handle, expected_kind, None);
        let entry = self.current_epochs.entry((handle, domain)).or_insert(0);
        *entry += 1;
        let epoch = *entry;
        AsyncToken::with_domain(handle, expected_kind, epoch, domain)
    }

    /// Registers an `AsyncToken` for a specific job ID.
    fn register_job_token(&mut self, job_id: usize, token: AsyncToken) {
        self.job_tokens.insert(job_id, token);
    }

    /// Retrieves the `AsyncToken` associated with a job ID, if any.
    #[must_use]
    pub(crate) fn job_token(&self, job_id: usize) -> Option<AsyncToken> {
        self.job_tokens.get(&job_id).copied()
    }

    /// Checks whether an `AsyncToken`'s epoch matches the current epoch for its handle and domain.
    #[must_use]
    pub(crate) fn is_token_current(&self, token: &AsyncToken) -> bool {
        self.current_epochs
            .get(&(token.handle, token.domain))
            .copied()
            .is_some_and(|current| current == token.epoch)
    }

    /// Cancels all tracked jobs for a given document handle and purges their epochs/tokens.
    pub(crate) fn cancel_jobs_for_handle(&mut self, handle: DocumentHandle) -> Vec<usize> {
        let matching_ids: Vec<usize> = self
            .job_tokens
            .iter()
            .filter(|(_, token)| token.handle == handle)
            .map(|(id, _)| *id)
            .collect();
        for &id in &matching_ids {
            self.cancel_job(id);
            self.job_tokens.remove(&id);
        }
        self.current_epochs.retain(|(h, _), _| *h != handle);
        if let Some((curr, _, _)) = self.active_handles.get(&handle.doc_id) {
            if *curr == handle {
                self.active_handles.remove(&handle.doc_id);
            }
        }
        matching_ids
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        // Signal cancellation to all jobs
        for job in self.jobs.values() {
            job.cancellation_token.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
