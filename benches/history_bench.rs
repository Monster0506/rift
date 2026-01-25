use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::character::Character;
use monster_rift::history::{EditOperation, EditTransaction, Position, UndoTree};
use std::hint::black_box;

fn history_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("history_operations");

    // Helper to create a dummy transaction
    let create_transaction = |i: usize| {
        let mut tx = EditTransaction::new(format!("Test edit {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, 0),
            text: vec![Character::from('a')],
            len: 1,
        });
        tx
    };

    group.bench_function("push_edit", |b| {
        b.iter_batched(
            || UndoTree::new(),
            |mut history| {
                // Push 100 edits
                for i in 0..100 {
                    history.push(create_transaction(i), None);
                }
                history
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("undo_redo_small", |b| {
        // Setup: history with 100 edits
        b.iter_batched(
            || {
                let mut h = UndoTree::new();
                for i in 0..100 {
                    h.push(create_transaction(i), None);
                }
                h
            },
            |mut history| {
                // Undo 50
                for _ in 0..50 {
                    black_box(history.undo());
                }
                // Redo 50
                for _ in 0..50 {
                    black_box(history.redo());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("compute_replay_path_deep", |b| {
        // Deep history to stress LCA finding
        b.iter_batched(
            || {
                let mut h = UndoTree::new();
                // Create linear history of 1000 items
                for i in 0..1000 {
                    h.push(create_transaction(i), None);
                }
                // Go back to edit 500
                let from_seq = h.current_seq();

                // We want to pathfind from Tip to Root.
                (h, from_seq, 0)
            },
            |(h, from, to)| {
                black_box(h.compute_replay_path(from, to).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, history_operations);
criterion_main!(benches);
