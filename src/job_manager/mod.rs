use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

pub mod jobs;

use std::any::Any;

/// Sealed trait for job payloads to ensure type safety.
pub trait JobPayload: Any + Send + std::fmt::Debug + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
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
}

/// Signal used to check if a job has been cancelled.
#[derive(Debug, Clone)]
pub struct CancellationSignal {
    cancelled: Arc<AtomicBool>,
}

impl CancellationSignal {
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
}

impl Job for Box<dyn Job> {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        (*self).run(id, sender, signal);
    }

    fn is_silent(&self) -> bool {
        (**self).is_silent()
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
        let cancellation_token = Arc::new(AtomicBool::new(false));
        let signal = CancellationSignal {
            cancelled: cancellation_token.clone(),
        };
        let silent = job.is_silent();
        let job_box = Box::new(job);

        let handle = thread::spawn(move || {
            // Signal start
            if sender.send(JobMessage::Started(id, silent)).is_ok() {
                job_box.run(id, sender, signal);
            }
        });

        self.jobs.insert(
            id,
            JobHandle {
                handle,
                state: JobState::Running,
                cancellation_token,
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
