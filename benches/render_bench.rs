use criterion::{criterion_group, criterion_main, Criterion};
use monster_rift::bench_api::buffer::TextBuffer;
use monster_rift::bench_api::error::RiftError;
use monster_rift::bench_api::mode::Mode;
use monster_rift::bench_api::render::{RenderState, RenderSystem};
use monster_rift::bench_api::state::State;
use monster_rift::bench_api::term::TerminalBackend;
use std::hint::black_box;

// Mock Terminal to avoid I/O overhead
pub struct MockTerminal {
    pub rows: u16,
    pub cols: u16,
}

impl MockTerminal {
    const fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }
}

impl TerminalBackend for MockTerminal {
    fn init(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn deinit(&mut self) {}

    fn poll(&mut self, _duration: std::time::Duration) -> Result<bool, RiftError> {
        Ok(false)
    }

    fn read_event(
        &mut self,
    ) -> Result<Option<monster_rift::bench_api::key::InputEvent>, RiftError> {
        Ok(None)
    }

    fn write(&mut self, _bytes: &[u8]) -> Result<(), RiftError> {
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn get_size(&self) -> Result<monster_rift::bench_api::term::Size, RiftError> {
        Ok(monster_rift::bench_api::term::Size {
            rows: self.rows,
            cols: self.cols,
        })
    }

    fn clear_screen(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn move_cursor(&mut self, _row: u16, _col: u16) -> Result<(), RiftError> {
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), RiftError> {
        Ok(())
    }

    fn set_cursor_shape(
        &mut self,
        _shape: monster_rift::bench_api::term::CursorShape,
    ) -> Result<(), RiftError> {
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
                "fn function_{i}() {{ println!(\"Hello World {i}\"); }}\n"
            ))
            .unwrap();
        }

        let state = State::new();
        let rows: u16 = 40;
        let cols: u16 = 120;

        let mut render_system = RenderSystem::new(rows.into(), cols.into());
        // Force full redraw setup
        render_system.resize(rows.into(), cols.into());

        let mock_term = MockTerminal::new(rows, cols);

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
                    injection_highlights: None,
                    skip_content: false,
                    cursor_row_offset: 0,
                    cursor_col_offset: 0,
                    cursor_viewport: None,
                    terminal_cursor: None,
                    custom_highlights: None,
                    git_gutter_colors: None,
                    annotation_styles: None,
                    annotation_adornments: None,
                    annotation_inline: None,
                    annotation_concealed: None,
                    terminal_cell_colors: None,
                    show_line_numbers: true,
                    display_map: None,
                    scroll_hint: None,
                    syntax_generation: 0,
                    annotations_revision: 0,
                    kind_registry_generation: 0,
                };

                // Measure the full render pass
                // Note: We access `rs` mutably, but `state` and `buf` are borrowed
                black_box(rs.render(&mut term, &render_state).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, render_loop);
criterion_main!(benches);
