use super::*;
use std::time::Duration;

#[derive(Debug)]
struct TestJob {
    duration_ms: u64,
    succeed: bool,
}

impl Job for TestJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>) {
        // Simulate work
        for i in 0..5 {
            if sender
                .send(JobMessage::Progress(id, i * 20, format!("Step {}", i + 1)))
                .is_err()
            {
                return; // Channel closed
            }
            thread::sleep(Duration::from_millis(self.duration_ms / 5));
        }

        if self.succeed {
            let _ = sender.send(JobMessage::Finished(id, false));
        } else {
            let _ = sender.send(JobMessage::Error(id, "Failed artificially".to_string()));
        }
    }
}

#[test]
fn test_job_lifecycle() {
    let mut manager = JobManager::new();
    let job = TestJob {
        duration_ms: 100,
        succeed: true,
    };

    let id = manager.spawn(job);
    let receiver = manager.receiver();

    // We should receive Started
    let msg = receiver.recv().unwrap();
    matches!(msg, JobMessage::Started(mid, _) if mid == id);

    // We should receive progress
    loop {
        let msg = receiver.recv().unwrap();
        match msg {
            JobMessage::Progress(mid, _, _) => assert_eq!(mid, id),
            JobMessage::Finished(mid, _) => {
                assert_eq!(mid, id);
                break;
            }
            _ => panic!("Unexpected message"),
        }
    }
}

#[test]
fn test_manager_state_update() {
    let mut manager = JobManager::new();
    let job = TestJob {
        duration_ms: 10,
        succeed: true,
    };
    let id = manager.spawn(job);

    // Consume messages
    thread::sleep(Duration::from_millis(50));
    while let Ok(msg) = manager.receiver().try_recv() {
        manager.update_job_state(&msg);
    }

    let job_handle = manager.jobs.get(&id).expect("Job should exist");
    assert_eq!(job_handle.state, JobState::Finished);

    // Test cleanup
    let cleaned = manager.cleanup_finished_jobs();
    assert!(cleaned.contains(&id));
    assert!(!manager.jobs.contains_key(&id));
}
