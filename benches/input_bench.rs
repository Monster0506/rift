use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::action::{Action, EditorAction, Motion};
use monster_rift::key::Key;
use monster_rift::keymap::{KeyContext, KeyMap};
use std::hint::black_box;

fn keymap_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("keymap_lookup");

    group.bench_function("lookup_global", |b| {
        let mut map = KeyMap::new();
        // Fill with some bindings
        for i in 0..100 {
            // Using char keys for simplicity
            let c = std::char::from_u32(32 + i as u32).unwrap_or('a');
            map.register(KeyContext::Global, Key::Char(c), Action::Noop);
        }

        b.iter(|| {
            black_box(map.get_action(KeyContext::Global, Key::Char('A')));
        })
    });

    group.bench_function("lookup_fallback", |b| {
        let mut map = KeyMap::new();
        // Register only global
        map.register(
            KeyContext::Global,
            Key::Char('j'),
            Action::Editor(EditorAction::Move(Motion::Down)),
        );

        // Lookup in 'Normal' context, expecting fallback to 'Global'
        b.iter(|| {
            black_box(map.get_action(KeyContext::Normal, Key::Char('j')));
        })
    });

    group.finish();
}

criterion_group!(benches, keymap_lookup);
criterion_main!(benches);
