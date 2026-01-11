use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

/// Sealed trait for job payloads to ensure type safety.
pub trait JobPayload: Send + std::fmt::Debug + 'static {}

/// Message sent from a background job to the editor.
#[derive(Debug)]
pub enum JobMessage {
    /// Job started with ID
    Started(usize),
    /// Progress update: Job ID, percentage (0-100), status message
    Progress(usize, u32, String),
    /// Job finished successfully
    Finished(usize),
    /// Job failed with error message
    Error(usize, String),
    /// Job cancelled (terminal state)
    Cancelled(usize),
    /// Custom payload for job-specific results
    Custom(usize, Box<dyn JobPayload>),
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
    /// * The job SHOULD check `sender.send(...)` results; if it fails, the editor is gone/cancelled, and the job SHOULD exit.
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>);
}

impl Job for Box<dyn Job> {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>) {
        (*self).run(id, sender);
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
        }
    }

    /// Spawn a new job.
    /// returns the Job ID.
    pub fn spawn<J: Job>(&mut self, job: J) -> usize {
        let id = self.next_job_id;
        self.next_job_id += 1;

        let sender = self.sender.clone();
        let job_box = Box::new(job);

        let handle = thread::spawn(move || {
            // Signal start
            if sender.send(JobMessage::Started(id)).is_ok() {
                job_box.run(id, sender);
            }
        });

        self.jobs.insert(
            id,
            JobHandle {
                handle,
                state: JobState::Running,
            },
        );

        id
    }

    /// Get the receiver to poll for messages.
    /// The editor should call `receiver.try_recv()` to get messages without blocking.
    pub fn receiver(&self) -> &Receiver<JobMessage> {
        &self.receiver
    }

    /// Update job state based on message.
    /// This should be called by the editor when it processes a message.
    pub fn update_job_state(&mut self, message: &JobMessage) {
        match message {
            JobMessage::Finished(id) => {
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
            ) {
                if job.handle.is_finished() {
                    finished_ids.push(*id);
                }
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
    /// Note: This only marks it as cancelled in the manager and drops the handle from our map if we wanted to enforce detachment,
    /// but since we need to join it, we can't force-kill a thread in Rust safe code.
    /// The job must cooperate by checking the channel.
    /// For now, we just mark state. Real cancellation requires the job to check an AtomicBool or channel.
    /// Since we are using channel disconnection as a signal (in Drop), explicit cancellation of a single job
    /// is harder without a per-job control channel.
    /// For this v0, we will assume generic "cancellation" is mostly "editor shutdown".
    /// If we need per-job cancellation, we would need to pass a cancellation token to `run`.
    pub fn cancel_job(&mut self, _id: usize) {
        // TODO: Implement per-job cancellation via AtomicBool or similar if needed.
        // For now, this is a placeholder.
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        // When JobManager is dropped (e.g. editor shutdown), the sender is dropped.
        // Jobs attempting to send messages will get an error, helping them exit.
        // We generally can't forcibly join all threads here without potentially blocking the UI thread (main thread)
        // for too long, but for correctness, we should try to allow them to exit.
        // However, if we block on join, a stuck job could hang the editor exit.
        // So we will just let them detach, but since the channel is closed, they should exit naturally.
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
