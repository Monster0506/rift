use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::bench_api::action::{Action, EditorAction, Motion};
use monster_rift::bench_api::key::Key;
use monster_rift::bench_api::keymap::{KeyContext, KeyMap};
use std::hint::black_box;

fn keymap_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("keymap_lookup");

    group.bench_function("lookup_global", |b| {
        let mut map = KeyMap::new();
        // Fill with some bindings
        for i in 0_u32..100 {
            // Using char keys for simplicity
            let c = std::char::from_u32(32 + i).unwrap_or('a');
            map.register(KeyContext::Global, Key::char(c), Action::Noop);
        }

        b.iter(|| {
            black_box(map.get_action(KeyContext::Global, Key::char('A')));
        });
    });

    group.bench_function("lookup_fallback", |b| {
        let mut map = KeyMap::new();
        // Register only global
        map.register(
            KeyContext::Global,
            Key::char('j'),
            Action::Editor(EditorAction::Move(Motion::Down)),
        );

        // Lookup in 'Normal' context, expecting fallback to 'Global'
        b.iter(|| {
            black_box(map.get_action(KeyContext::Normal, Key::char('j')));
        });
    });

    group.finish();
}

criterion_group!(benches, keymap_lookup);
criterion_main!(benches);
