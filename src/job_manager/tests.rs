use super::*;
use std::time::Duration;

#[derive(Debug)]
struct TestJob {
    duration_ms: u64,
    succeed: bool,
}

#[derive(Debug)]
struct NamedJob {
    name: &'static str,
    silent: bool,
}

impl Job for NamedJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, _signal: CancellationSignal) {
        let _ = sender.send(JobMessage::Finished(id, self.silent));
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn is_silent(&self) -> bool {
        self.silent
    }
}

impl Job for TestJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        // Simulate work
        for i in 0..5 {
            if signal.is_cancelled() {
                let _ = sender.send(JobMessage::Cancelled(id));
                return;
            }
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

#[test]
fn test_job_cancellation() {
    let mut manager = JobManager::new();
    // Long running job
    let job = TestJob {
        duration_ms: 1000,
        succeed: true,
    };
    let id = manager.spawn(job);

    // Wait a bit then cancel
    thread::sleep(Duration::from_millis(10));
    manager.cancel_job(id);

    let receiver = manager.receiver();
    // Should get Started
    let _ = receiver.recv();

    // Drain until cancelled or finished
    let mut cancelled = false;
    while let Ok(msg) = receiver.recv_timeout(Duration::from_millis(200)) {
        match msg {
            JobMessage::Cancelled(mid) if mid == id => {
                cancelled = true;
                break;
            }
            _ => {}
        }
    }

    assert!(cancelled, "Job should have received cancelled message");
}

#[test]
fn test_job_default_name() {
    let job = TestJob {
        duration_ms: 0,
        succeed: true,
    };
    assert_eq!(job.name(), "job"); // default from trait
}

#[test]
fn test_named_job_name() {
    let job = NamedJob {
        name: "file-save",
        silent: false,
    };
    assert_eq!(job.name(), "file-save");
}

#[test]
fn test_job_name_stored_in_handle() {
    let mut manager = JobManager::new();
    let id = manager.spawn(NamedJob {
        name: "syntax-parse",
        silent: true,
    });

    // Drain the Started message so the handle is registered
    let _ = manager.receiver().recv_timeout(Duration::from_millis(200));

    assert_eq!(manager.jobs.get(&id).unwrap().name, "syntax-parse");
}

#[test]
fn test_job_name_accessor() {
    let mut manager = JobManager::new();
    let id = manager.spawn(NamedJob {
        name: "fs-copy",
        silent: false,
    });

    // Drain messages
    let _ = manager.receiver().recv_timeout(Duration::from_millis(200));
    let _ = manager.receiver().recv_timeout(Duration::from_millis(200));

    assert_eq!(manager.job_name(id), "fs-copy");
}

#[test]
fn test_job_name_unknown_id_returns_default() {
    let manager = JobManager::new();
    assert_eq!(manager.job_name(9999), "job");
}

#[test]
fn test_silent_job_name_preserved() {
    let mut manager = JobManager::new();
    let id = manager.spawn(NamedJob {
        name: "cache-warming",
        silent: true,
    });

    let _ = manager.receiver().recv_timeout(Duration::from_millis(200));

    let handle = manager.jobs.get(&id).unwrap();
    assert!(handle.silent);
    assert_eq!(handle.name, "cache-warming");
}

#[test]
fn test_multiple_jobs_have_independent_names() {
    let mut manager = JobManager::new();
    let id1 = manager.spawn(NamedJob {
        name: "file-load",
        silent: true,
    });
    let id2 = manager.spawn(NamedJob {
        name: "undotree-render",
        silent: true,
    });

    // Drain
    for _ in 0..4 {
        let _ = manager.receiver().recv_timeout(Duration::from_millis(100));
    }

    assert_eq!(manager.job_name(id1), "file-load");
    assert_eq!(manager.job_name(id2), "undotree-render");
}

#[test]
fn test_cache_warming_job_name() {
    use crate::buffer::rope::PieceTable;
    use crate::job_manager::jobs::cache_warming::CacheWarmingJob;
    let job = CacheWarmingJob::new(PieceTable::new(vec![]), 0);
    assert_eq!(job.name(), "cache-warming");
    assert!(job.is_silent());
}

#[test]
fn test_completion_job_name() {
    use crate::job_manager::jobs::completion::CompletionJob;
    let job = CompletionJob {
        input: String::new(),
        current_settings: None,
        current_doc_options: None,
        plugin_commands: vec![],
        line_count: 0,
        buf_words: vec![],
    };
    assert_eq!(job.name(), "completion");
    assert!(job.is_silent());
}

#[test]
fn test_directory_list_job_name() {
    use crate::job_manager::jobs::explorer::DirectoryListJob;
    let job = DirectoryListJob::new(1, std::path::PathBuf::from("/tmp"), false);
    assert_eq!(job.name(), "directory-list");
    assert!(job.is_silent());
}

#[test]
fn test_fs_copy_job_name() {
    use crate::job_manager::jobs::fs::FsCopyJob;
    let job = FsCopyJob::new(
        std::path::PathBuf::from("/a"),
        std::path::PathBuf::from("/b"),
    );
    assert_eq!(job.name(), "fs-copy");
    assert!(!job.is_silent());
}

#[test]
fn test_fs_move_job_name() {
    use crate::job_manager::jobs::fs::FsMoveJob;
    let job = FsMoveJob::new(
        std::path::PathBuf::from("/a"),
        std::path::PathBuf::from("/b"),
    );
    assert_eq!(job.name(), "fs-move");
}

#[test]
fn test_fs_delete_job_name() {
    use crate::job_manager::jobs::fs::FsDeleteJob;
    let job = FsDeleteJob::new(std::path::PathBuf::from("/a"));
    assert_eq!(job.name(), "fs-delete");
}

#[test]
fn test_fs_create_job_name() {
    use crate::job_manager::jobs::fs::FsCreateJob;
    let job = FsCreateJob::new(std::path::PathBuf::from("/a/b.txt"), false);
    assert_eq!(job.name(), "fs-create");
}

#[test]
fn test_fs_batch_delete_job_name() {
    use crate::job_manager::jobs::fs::FsBatchDeleteJob;
    let job = FsBatchDeleteJob::new(vec![]);
    assert_eq!(job.name(), "fs-batch-delete");
}
