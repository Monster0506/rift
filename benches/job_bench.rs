use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::job_manager::{CancellationSignal, Job, JobManager, JobMessage};
use std::hint::black_box;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct NoOpJob;

impl Job for NoOpJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, _signal: CancellationSignal) {
        // Just finish immediately
        let _ = sender.send(JobMessage::Finished(id, true));
    }
}

fn job_system_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("job_system");

    group.bench_function("spawn_noop_job", |b| {
        let mut manager = JobManager::new();
        b.iter(|| {
            black_box(manager.spawn(NoOpJob));
        })
    });

    group.bench_function("cleanup_finished_jobs", |b| {
        b.iter_batched(
            || {
                let mut manager = JobManager::new();
                // Spawn 100 jobs that finish instantly
                for _ in 0..100 {
                    manager.spawn(NoOpJob);
                }
                // Wait a bit for them to likely finish (in real bench this is flaky but best effort for unit)
                thread::sleep(Duration::from_millis(50));

                // We need to drain the receiver to update state
                while let Ok(msg) = manager.receiver().try_recv() {
                    manager.update_job_state(&msg);
                }
                manager
            },
            |mut manager| {
                black_box(manager.cleanup_finished_jobs());
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, job_system_throughput);
criterion_main!(benches);
