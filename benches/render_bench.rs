use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use monster_rift::buffer::TextBuffer;
use monster_rift::error::RiftError;
use monster_rift::mode::Mode;
use monster_rift::render::{RenderState, RenderSystem};
use monster_rift::state::State;
use monster_rift::term::TerminalBackend;
use monster_rift::viewport::Viewport;
use std::hint::black_box;

// Mock Terminal to avoid I/O overhead
pub struct MockTerminal {
    pub rows: u16,
    pub cols: u16,
}

impl MockTerminal {
    fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }
}

impl TerminalBackend for MockTerminal {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, String> {
        Ok(false)
    }

    fn read_key(&mut self) -> Result<Option<monster_rift::key::Key>, String> {
        Ok(None)
    }

    fn write(&mut self, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn get_size(&self) -> Result<monster_rift::term::Size, String> {
        Ok(monster_rift::term::Size {
            rows: self.rows,
            cols: self.cols,
        })
    }

    fn clear_screen(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn move_cursor(&mut self, _row: u16, _col: u16) -> Result<(), String> {
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), String> {
        Ok(())
    }
}

fn render_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_loop");

    // Setup typical editor state
    let setup_render = || {
        let mut buf = TextBuffer::new(1024 * 1024).unwrap();
        // Fill with some realistic code-like content
        for i in 0..10_000 {
            buf.insert_str(&format!(
                "fn function_{}() {{ println!(\"Hello World {}\"); }}\n",
                i, i
            ))
            .unwrap();
        }

        let state = State::new();
        let rows = 40;
        let cols = 120;

        let mut render_system = RenderSystem::new(rows, cols);
        // Force full redraw setup
        render_system.resize(rows, cols);

        let mock_term = MockTerminal::new(rows as u16, cols as u16);

        (render_system, buf, state, mock_term)
    };

    group.bench_function("render_full_frame", |b| {
        b.iter_batched(
            setup_render,
            |(mut rs, buf, state, mut term)| {
                // Construct RenderState (ephemeral)
                let render_state = RenderState {
                    buf: &buf,
                    current_mode: Mode::Normal,
                    pending_key: None,
                    pending_count: 0,
                    state: &state,
                    needs_clear: false,
                    tab_width: 4,
                    highlights: None,
                    capture_map: None,
                    modal: None,
                };

                // Measure the full render pass
                // Note: We access `rs` mutably, but `state` and `buf` are borrowed
                black_box(rs.render(&mut term, render_state).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, render_loop);
criterion_main!(benches);
