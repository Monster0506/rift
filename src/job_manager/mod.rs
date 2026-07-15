use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

pub mod jobs;

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
    fn new(permits: usize) -> Self {
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
        let mut permits = self.state.lock().unwrap();
        *permits += 1;
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
#[macro_export]
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
    Error(usize, String),
    /// Job cancelled (terminal state)
    Cancelled(usize),
    /// Custom payload for job-specific results
    Custom(usize, Box<dyn JobPayload>),
    /// Terminal output data (DocumentId, Data)
    TerminalOutput(crate::document::DocumentId, Vec<u8>),
    /// Terminal process exit (DocumentId)
    TerminalExit(crate::document::DocumentId),
}

/// Sends a job's successful result, then its Finished message. The common
/// two-message tail of a job's `run()` once it has produced a payload.
pub fn send_job_result(sender: &Sender<JobMessage>, id: usize, payload: Box<dyn JobPayload>) {
    let _ = sender.send(JobMessage::Custom(id, payload));
    let _ = sender.send(JobMessage::Finished(id, true));
}

/// Signal used to check if a job has been cancelled.
#[derive(Debug, Clone)]
pub struct CancellationSignal {
    cancelled: Arc<AtomicBool>,
}

impl CancellationSignal {
    /// Create a signal already in the given cancelled state.
    pub fn new(cancelled: bool) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(cancelled)),
        }
    }

    /// Create a signal that is never cancelled, for non-job callers.
    pub fn new_uncancelled() -> Self {
        Self::new(false)
    }

    /// Check if the job has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// State of a background job
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Finished,
    Failed,
    Cancelled,
}

/// Handle to a running job
pub struct JobHandle {
    pub handle: JoinHandle<()>,
    pub state: JobState,
    pub cancellation_token: Arc<AtomicBool>,
    /// Whether the job opted out of notifications
    pub silent: bool,
    /// Descriptive name for the job type
    pub name: &'static str,
}

/// Trait defining a background job.
/// Jobs must be Send + 'static to be moved into a thread.
pub trait Job: Send + std::fmt::Debug + 'static {
    /// Run the job.
    ///
    /// # Arguments
    /// * `id` - The unique ID assigned to this job.
    /// * `sender` - Channel to send messages back to the editor.
    ///
    /// # Invariants
    /// * The job MUST NOT access global editor state.
    /// * The job SHOULD check `sender.send(...)` results AND `cancellation_signal.is_cancelled()`.
    /// * If cancelled, the job SHOULD exit as soon as possible.
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal);

    /// Whether this job should trigger notifications in the editor.
    fn is_silent(&self) -> bool {
        false
    }

    /// Descriptive name for this job type.
    fn name(&self) -> &'static str {
        "job"
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
}

/// Extract a readable message from a caught panic payload.
fn panic_payload_to_string(payload: &Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Manages background jobs.
pub struct JobManager {
    /// Sender to clone for new jobs
    sender: Sender<JobMessage>,
    /// Receiver for the editor to poll
    receiver: Receiver<JobMessage>,
    /// Active jobs map
    jobs: HashMap<usize, JobHandle>,
    /// Counter for generating job IDs
    next_job_id: usize,
    /// Caps how many job bodies may run concurrently.
    concurrency_limiter: Arc<Semaphore>,
}

impl JobManager {
    /// Create a new JobManager
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            jobs: HashMap::new(),
            next_job_id: 1,
            concurrency_limiter: Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS)),
        }
    }

    /// Spawn a new job.
    /// returns the Job ID.
    pub fn spawn<J: Job>(&mut self, job: J) -> usize {
        let id = self.next_job_id;
        self.next_job_id += 1;

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
                        let _ =
                            sender.send(JobMessage::Error(id, format!("job panicked: {details}")));
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
    pub fn receiver(&self) -> &Receiver<JobMessage> {
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

    /// Clean up finished/failed/cancelled jobs.
    /// This joins the threads to release resources.
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
        }

        finished_ids
    }

    /// Cancel a specific job.
    /// This sets the cancellation flag and marks the state as Cancelled.
    /// The job thread is expected to notice the flag and exit.
    pub fn cancel_job(&mut self, id: usize) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.cancellation_token.store(true, Ordering::Relaxed);
            job.state = JobState::Cancelled;
        }
    }

    /// Returns true if the given job was spawned with `is_silent() == true`.
    pub fn is_job_silent(&self, id: usize) -> bool {
        self.jobs.get(&id).map(|h| h.silent).unwrap_or(false)
    }

    /// Returns the descriptive name of the given job, or `"job"` if not found.
    pub fn job_name(&self, id: usize) -> &'static str {
        self.jobs.get(&id).map(|h| h.name).unwrap_or("job")
    }

    /// Returns the current state of a job, if it is still tracked.
    pub fn job_state(&self, id: usize) -> Option<JobState> {
        self.jobs.get(&id).map(|h| h.state)
    }

    /// Returns the number of jobs ever spawned so far (monotonic counter).
    pub fn total_spawned(&self) -> usize {
        self.next_job_id - 1
    }

    /// Whether any spawned job's OS thread is still executing, checked via
    /// the thread handle so it's accurate even if the caller isn't draining.
    pub fn any_job_thread_alive(&self) -> bool {
        self.jobs.values().any(|h| !h.handle.is_finished())
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
