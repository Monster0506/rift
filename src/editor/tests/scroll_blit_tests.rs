use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn dirty_row_scroll_blit_matches_a_fresh_full_render_at_every_step() {
    let paragraph = "The quick brown fox jumps over the lazy dog again and again near the riverbank while the sun sets slowly behind the distant hills. ";
    let mut text = String::new();
    for i in 0..80 {
        text.push_str(&format!("line {i}: {paragraph}\n"));
    }

    let steps: &[&str] = &[
        "j",
        "jjjjj",
        "jjjjj",
        "jjjjj",
        "kk",
        "G",
        "gg",
        "jjjjjjjjjjjjjjjjjjjj",
        "/fox<CR>",
        "n",
        "n",
        "kkkkkkkkk",
        "G",
        "jjj",
        "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
    ];

    let build_to = |n: usize| -> Editor<MockTerminal> {
        let mut editor = create_editor_sized(20, 80);
        load_text(&mut editor, &text);
        for step in &steps[..n] {
            feed_keys(&mut editor, step);
        }
        editor
    };

    let mut live = build_to(0);
    let mut any_blit_engaged = false;

    for (i, step) in steps.iter().enumerate() {
        feed_keys(&mut live, step);
        let live_output = render_ascii(&mut live);
        if live.render_system.content_blit_key.is_some() {
            any_blit_engaged = true;
        }

        let mut fresh = build_to(i + 1);
        let fresh_output = render_ascii(&mut fresh);

        assert_eq!(
            live_output, fresh_output,
            "step {i} (\"{step}\"): incrementally-scrolled (blit-eligible) render \
             diverged from a fresh full render at the same cursor position"
        );
    }

    assert!(
        any_blit_engaged,
        "test setup problem: the blit path never engaged across any step - \
         this test would pass trivially without actually exercising it"
    );
}

#[cfg(feature = "treesitter")]
fn content_cell_grid(editor: &mut Editor<MockTerminal>) -> Vec<Vec<crate::layer::Cell>> {
    editor.update_and_render().unwrap();
    let layer = editor
        .render_system
        .compositor
        .get_layer(crate::layer::LayerPriority::CONTENT)
        .expect("content layer exists after a render");
    let rows = layer.rows();
    let cols = layer.cols();
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    layer
                        .get_cell(r, c)
                        .cloned()
                        .unwrap_or_else(crate::layer::Cell::empty)
                })
                .collect()
        })
        .collect()
}

#[cfg(feature = "treesitter")]
fn assert_content_grids_match(
    live: &[Vec<crate::layer::Cell>],
    fresh: &[Vec<crate::layer::Cell>],
    step: usize,
    label: &str,
) {
    assert_eq!(
        live.len(),
        fresh.len(),
        "step {step} (\"{label}\"): content-layer row count mismatch"
    );
    for (r, (lrow, frow)) in live.iter().zip(fresh.iter()).enumerate() {
        assert_eq!(
            lrow.len(),
            frow.len(),
            "step {step} (\"{label}\"): row {r} col count mismatch"
        );
        for (c, (lc, fc)) in lrow.iter().zip(frow.iter()).enumerate() {
            assert_eq!(
                lc, fc,
                "step {step} (\"{label}\"): content-layer cell (color/attrs included) \
                 mismatch at row {r} col {c}"
            );
        }
    }
}

#[cfg(feature = "treesitter")]
#[test]
fn non_wrap_dirty_row_scroll_blit_matches_a_fresh_full_render_at_every_step() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut lines: Vec<String> = Vec::new();
    lines.push("fn build() -> i32 {".to_string());
    for i in 0..20 {}
    for _ in 0..40 {
        lines.push(String::new());
    }
    lines.push(format!("A".repeat(150)));
    lines.push("    x0 + x1".to_string());
    lines.push("}".to_string());
    let text = lines.join("\n") + "\n";

    let leading_anchor = text
        .find("x10 = 10")
        .expect("marker line present in fixture");

    let steps: &[&str] = &[
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "j",
        "j",
        "j",
        "kk",
        "jj",
        "j",
        "G",
        "gg",
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "j",
        "j",
        "/x15<CR>",
        "n",
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "j",
        "kkkkk",
    ];

    let build_to = |n: usize| -> Editor<MockTerminal> {
        let mut editor = create_editor_sized(20, 80);
        editor.state.settings.soft_wrap = false;
        load_text(&mut editor, &text);

        let loader = editor.language_loader.clone();
        let loaded = loader.load_language("rust").expect("rust grammar");
        let highlights = loader
            .load_query("rust", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
        editor.active_document().set_syntax(syntax);
        editor.do_incremental_syntax_parse();
        drain_jobs_until_idle(&mut editor);
        ensure_syntax_parsed(&mut editor);

        editor.active_document().annotations.add(
            Annotation::new(
                Kind::new("ui.x"),
                Anchor::point(leading_anchor),
                AnnotationOwner::User,
            )
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new(">>", Placement::Leading)),
            ),
        );

        for step in &steps[..n] {
            feed_keys(&mut editor, step);
        }
        editor
    };

    let mut live = build_to(0);
    let mut any_blit_engaged = false;
    #[cfg(feature = "perf_instrumentation")]
    let skipped_before = crate::perf::row_paint_counts().1;

    for (i, step) in steps.iter().enumerate() {
        feed_keys(&mut live, step);
        let live_grid = content_cell_grid(&mut live);
        if live.render_system.content_blit_key.is_some() {
            any_blit_engaged = true;
        }

        let mut fresh = build_to(i + 1);
        let fresh_grid = content_cell_grid(&mut fresh);

        assert_content_grids_match(&live_grid, &fresh_grid, i, step);
    }

    assert!(
        any_blit_engaged,
        "test setup problem: the blit path never engaged across any step - \
         this test would pass trivially without actually exercising it"
    );
    #[cfg(feature = "perf_instrumentation")]
    assert!(
        crate::perf::row_paint_counts().1 > skipped_before,
        "test setup problem: no non-wrap row was ever skipped via the scroll \
         blit across any step - this test would pass trivially without \
         actually exercising the row-skip path"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn wrap_dirty_row_scroll_blit_matches_a_fresh_full_render_on_densely_highlighted_document() {
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut lines: Vec<String> = vec![
        "use std::collections::HashMap;".to_string(),
        String::new(),
        "pub struct Cache {".to_string(),
        "    capacity: usize,".to_string(),
        "}".to_string(),
        String::new(),
        "impl Cache {".to_string(),
    ];
    for i in 0..12 {
        lines.push(format!());
        lines.push(format!(
            "    pub fn insert_{i}(&mut self, key: u32, value: String) -> bool {{"
        ));
        lines.push(format!());
        lines.push("        }".to_string());
        lines.push(format!());
        lines.push("        true".to_string());
        lines.push("    }".to_string());
        lines.push(String::new());
    }
    lines.push(format!("x".repeat(150)));
    lines.push("}".to_string());
    let text = lines.join("\n") + "\n";

    let steps: &[&str] = &[
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "j",
        "j",
        "j",
        "kk",
        "jj",
        "j",
        "G",
        "gg",
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "j",
        "j",
        "/insert_20<CR>",
        "n",
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
        "j",
        "kkkkk",
    ];

    let build_to = |n: usize| -> Editor<MockTerminal> {
        let mut editor = create_editor_sized(20, 80);
        load_text(&mut editor, &text);

        let loader = editor.language_loader.clone();
        let loaded = loader.load_language("rust").expect("rust grammar");
        let highlights = loader
            .load_query("rust", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
        editor.active_document().set_syntax(syntax);
        editor.do_incremental_syntax_parse();
        drain_jobs_until_idle(&mut editor);
        ensure_syntax_parsed(&mut editor);

        for step in &steps[..n] {
            feed_keys(&mut editor, step);
        }
        editor
    };

    let mut live = build_to(0);
    let mut any_blit_engaged = false;
    #[cfg(feature = "perf_instrumentation")]
    let (painted_before, skipped_before) = crate::perf::row_paint_counts();

    for (i, step) in steps.iter().enumerate() {
        feed_keys(&mut live, step);
        let live_grid = content_cell_grid(&mut live);
        if live.render_system.content_blit_key.is_some() {
            any_blit_engaged = true;
        }

        let mut fresh = build_to(i + 1);
        let fresh_grid = content_cell_grid(&mut fresh);

        assert_content_grids_match(&live_grid, &fresh_grid, i, step);
    }

    assert!(
        any_blit_engaged,
        "test setup problem: the blit path never engaged across any step - \
         this test would pass trivially without actually exercising it"
    );
    #[cfg(feature = "perf_instrumentation")]
    {
        let (painted_after, skipped_after) = crate::perf::row_paint_counts();
        let (painted, skipped) = (
            painted_after - painted_before,
            skipped_after - skipped_before,
        );
        let total = painted + skipped;
        eprintln!(
            "wrap densely-highlighted row-skip rate: painted={painted} skipped={skipped} \
             total={total} rate={:.1}%",
            100.0 * skipped as f64 / total.max(1) as f64
        );
        assert!(
            skipped > 0,
            "test setup problem: no wrap-mode row was ever skipped via the scroll \
             blit across any step on a densely-highlighted document - this test \
             would pass trivially without actually exercising the row-skip path"
        );
    }
}

#[cfg(feature = "treesitter")]
#[test]
fn scroll_blit_repaints_when_annotations_or_theme_change_mid_scroll() {
    use crate::annotations::{
        Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride,
    };
    use crate::color::theme::SyntaxColors;
    use crate::color::Color;
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut lines: Vec<String> = Vec::new();
    for i in 0..60 {
        lines.push(format!());
    }
    let text = lines.join("\n") + "\n";
    let style_anchor = text.find("value_5:").expect("marker line present");

    enum Step {
        Keys(&'static str),
        StyleValue5,
        SwapTheme,
    }

    let steps: &[Step] = &[
        Step::Keys("jjjjjjjjjj"),
        Step::Keys("jjjjj"),
        Step::StyleValue5,
        Step::Keys("jjjjj"),
        Step::SwapTheme,
        Step::Keys("jjjjj"),
        Step::Keys("kkkkkkkkkkkkkkkkkkkkk"),
    ];

    let apply_step = |editor: &mut Editor<MockTerminal>, step: &Step| match step {
        Step::Keys(seq) => feed_keys(editor, seq),
        Step::StyleValue5 => {
            editor.active_document().annotations.add(
                Annotation::new(
                    Kind::new("ui.mid_scroll_test"),
                    Anchor::range(style_anchor, style_anchor + 7),
                    AnnotationOwner::User,
                )
                .with_presentation(Presentation::with_style(StyleOverride {
                    fg: Some(Color::Red),
                    ..Default::default()
                })),
            );
        }
        Step::SwapTheme => {
            editor.state.settings.theme = Some("mid-scroll-alt".to_string());
            editor.state.settings.syntax_colors =
                Some(SyntaxColors::from_base_colors(&[("comment", Color::Green)]));
        }
    };

    let build_to = |n: usize| -> Editor<MockTerminal> {
        let mut editor = create_editor_sized(20, 80);
        load_text(&mut editor, &text);

        let loader = editor.language_loader.clone();
        let loaded = loader.load_language("rust").expect("rust grammar");
        let highlights = loader
            .load_query("rust", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
        editor.active_document().set_syntax(syntax);
        editor.do_incremental_syntax_parse();
        drain_jobs_until_idle(&mut editor);
        ensure_syntax_parsed(&mut editor);

        for step in &steps[..n] {
            apply_step(&mut editor, step);
        }
        editor
    };

    let mut live = build_to(0);
    for (i, step) in steps.iter().enumerate() {
        apply_step(&mut live, step);
        let live_grid = content_cell_grid(&mut live);

        let mut fresh = build_to(i + 1);
        let fresh_grid = content_cell_grid(&mut fresh);

        assert_content_grids_match(&live_grid, &fresh_grid, i, "mid-scroll mutation");
    }
}
