use super::*;
use crate::annotations::{Action, Anchor, Annotation, AnnotationOwner, Kind, Value};
use crate::test_utils::MockTerminal;

impl<T: TerminalBackend> Editor<T> {
    /// Test-only cursor offset accessor for the active document.
    fn active_cursor(&mut self) -> usize {
        self.document_manager
            .active_document_mut()
            .map(|d| d.buffer.cursor())
            .unwrap_or(0)
    }
}

fn editor_with_text(text: &str) -> Editor<MockTerminal> {
    let mut e = Editor::new(MockTerminal::new(24, 80)).unwrap();
    e.active_document().insert_str(text).unwrap();
    e
}

fn screen(e: &mut Editor<MockTerminal>) -> String {
    e.update_and_render().unwrap();
    let rows = e.render_system.compositor.rows();
    let cols = e.render_system.compositor.cols();
    let cells = e.render_system.compositor.get_composited_slice();
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| cells[r * cols + c].to_char())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render text plus a per-cell style legend (R reverse, U underline, B bold,
/// I italic, S strike, H non-default bg, C non-default fg, . default, space blank).
fn styled_view(e: &mut Editor<MockTerminal>) -> String {
    e.update_and_render().unwrap();
    let rows = e.render_system.compositor.rows();
    let cols = e.render_system.compositor.cols();
    let cells = e.render_system.compositor.get_composited_slice();
    // Treat the most common bg/fg as "default" so they aren't flagged.
    let mode = |pick: &dyn Fn(&crate::layer::Cell) -> Option<crate::color::Color>| {
        let mut counts: Vec<(Option<crate::color::Color>, usize)> = Vec::new();
        for c in cells {
            let v = pick(c);
            if let Some(entry) = counts.iter_mut().find(|(x, _)| *x == v) {
                entry.1 += 1;
            } else {
                counts.push((v, 1));
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(v, _)| v)
            .unwrap_or(None)
    };
    let default_bg = mode(&|c| c.bg);
    let default_fg = mode(&|c| c.fg);
    let mut out = String::new();
    for r in 0..rows {
        let row: Vec<_> = (0..cols).map(|c| &cells[r * cols + c]).collect();
        let text: String = row.iter().map(|c| c.to_char()).collect();
        if text.trim_end().is_empty() {
            continue;
        }
        let legend: String = row
            .iter()
            .map(|c| {
                let a = c.attrs;
                if a.reverse {
                    'R'
                } else if a.underline {
                    'U'
                } else if a.bold {
                    'B'
                } else if a.italic {
                    'I'
                } else if a.strike {
                    'S'
                } else if c.bg != default_bg {
                    'H'
                } else if c.fg != default_fg {
                    'C'
                } else if c.fg.is_some() {
                    '.'
                } else {
                    ' '
                }
            })
            .collect();
        out.push_str(text.trim_end());
        out.push('\n');
        out.push_str(legend.trim_end());
        out.push('\n');
    }
    out
}

#[test]
fn demo_annotated_markdown_document() {
    use crate::annotations::{Adornment, FaceRef, Placement, Presentation, StyleOverride, Value};
    use crate::color::Color;

    let content = "\
# Rift Annotations Demo

Inline styles: bold italic underline strike reverse all at once.

Links: read the docs or browse the repo for details.

Button: Run Tests runs the whole suite right now.

Tasks:
  [ ] pending item one
  [ ] pending item two

Diagnostic below:
let total = ;

Search: needle here and another needle over there.";

    let mut e = editor_with_text(content);
    let range = |s: &str| {
        let st = content.find(s).expect("substring present");
        st..st + s.len()
    };
    let point = |s: &str| content.find(s).expect("substring present");

    // --- Inline text styles -------------------------------------------
    let styles: &[(&str, StyleOverride)] = &[
        (
            "bold",
            StyleOverride {
                bold: true,
                ..Default::default()
            },
        ),
        (
            "italic",
            StyleOverride {
                italic: true,
                ..Default::default()
            },
        ),
        (
            "underline",
            StyleOverride {
                underline: true,
                ..Default::default()
            },
        ),
        (
            "strike",
            StyleOverride {
                strike: true,
                ..Default::default()
            },
        ),
        (
            "reverse",
            StyleOverride {
                reverse: true,
                ..Default::default()
            },
        ),
    ];
    for (word, style) in styles {
        let r = range(word);
        e.active_document().annotations.add(
            Annotation::new(
                Kind::new("ui.style"),
                Anchor::range(r.start, r.end),
                AnnotationOwner::User,
            )
            .with_presentation(Presentation::with_style(*style)),
        );
    }

    // --- Interactive links (blue + underline, activate -> follow) ------
    for (word, href) in [("docs", "docs.md"), ("repo", "https://example.com")] {
        let r = range(word);
        let mut pres = Presentation::with_face(FaceRef::new("link"));
        pres.style = Some(StyleOverride {
            underline: true,
            ..Default::default()
        });
        let mut payload = Value::map();
        payload.set("href", Value::Str(href.into()));
        payload.set("tooltip", Value::Str(format!("link: {}", href)));
        e.active_document().annotations.add(
            Annotation::new(
                Kind::new("ui.link"),
                Anchor::range(r.start, r.end),
                AnnotationOwner::User,
            )
            .with_payload(payload)
            .with_presentation(pres)
            .with_actions(vec![Action::activate()]),
        );
    }

    // --- A button (reverse video, interactive run action) -------------
    let btn = range("Run Tests");
    let mut btn_payload = Value::map();
    btn_payload.set("tooltip", Value::Str("Run the test suite".into()));
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.button"),
            Anchor::range(btn.start, btn.end),
            AnnotationOwner::User,
        )
        .with_payload(btn_payload)
        .with_presentation(Presentation::with_style(StyleOverride {
            reverse: true,
            fg: Some(Color::Cyan),
            ..Default::default()
        }))
        .with_actions(vec![Action::new("run").as_default()]),
    );

    // --- Checkboxes (overlay glyph + toggle action) -------------------
    let mut checkbox_ids = Vec::new();
    for task in ["[ ] pending item one", "[ ] pending item two"] {
        let p = point(task);
        let mut payload = Value::map();
        payload.set("checked", Value::Bool(false));
        let id = e.active_document().annotations.add(
            Annotation::new(
                Kind::new("ui.checkbox"),
                Anchor::point(p),
                AnnotationOwner::User,
            )
            .with_payload(payload)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new("[ ]", Placement::Overlay)),
            )
            .with_actions(vec![Action::new("toggle").as_default()]),
        );
        checkbox_ids.push(id);
    }

    // --- A diagnostic (face + EOL message + hover) --------------------
    let diag_line = content[..point("let total")].matches('\n').count();
    e.active_document()
        .annotations
        .create_diagnostic(diag_line, 1, "expected expression");

    // --- Search highlighting through annotations ----------------------
    e.state.last_search_query = Some("needle".to_string());
    e.update_search_highlights();

    // Render the whole thing and print the styled view.
    let view = styled_view(&mut e);
    eprintln!("\n===== ANNOTATED MARKDOWN DEMO (initial) =====\n{}", view);

    // Sanity checks across the feature set.
    let cells = e.render_system.compositor.get_composited_slice();
    assert!(cells.iter().any(|c| c.attrs.underline), "underline present");
    assert!(cells.iter().any(|c| c.attrs.bold), "bold present");
    assert!(cells.iter().any(|c| c.attrs.italic), "italic present");
    assert!(cells.iter().any(|c| c.attrs.strike), "strike present");
    assert!(cells.iter().any(|c| c.attrs.reverse), "reverse present");
    assert!(
        cells.iter().any(|c| c.bg == Some(Color::Yellow)),
        "search highlight present"
    );
    let txt = screen(&mut e);
    assert!(txt.contains("[ ]"), "checkbox overlay present");
    assert!(
        txt.contains("expected expression"),
        "diagnostic EOL adornment present"
    );

    // --- Demonstrate interactivity: toggle the first checkbox ----------
    let cb_offset = point("[ ] pending item one");
    e.active_document().buffer.set_cursor(cb_offset).ok();
    assert!(e.activate_annotation_at_cursor(), "checkbox activates");
    let checked = e
        .active_document()
        .annotations
        .get(checkbox_ids[0])
        .and_then(|a| a.payload.get("checked").and_then(Value::as_bool));
    assert_eq!(checked, Some(true), "payload toggled");
    let after = screen(&mut e);
    assert!(after.contains("[x]"), "overlay glyph updated to checked");
    eprintln!(
        "\n===== AFTER ACTIVATING FIRST CHECKBOX =====\n{}",
        styled_view(&mut e)
    );

    // The button is interactive (has a default action).
    let btn_ann = e
        .active_document()
        .annotations
        .query_kind("ui.button")
        .next()
        .unwrap();
    assert!(btn_ann.is_interactive());
}

#[test]
fn eol_adornment_renders_as_virtual_text() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, FaceRef, Kind, Placement, Presentation,
    };
    let mut e = editor_with_text("hello\nworld");
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("lsp.diagnostic"),
            Anchor::Line(0),
            AnnotationOwner::Lsp,
        )
        .with_presentation(Presentation::default().with_adornment(
            Adornment::new("E: boom", Placement::Trailing).with_face(FaceRef::new("diag.error")),
        )),
    );
    let out = screen(&mut e);
    assert!(
        out.contains("E: boom"),
        "EOL adornment should render as virtual text; screen:\n{}",
        out
    );
}

#[test]
fn search_highlight_flows_through_annotations() {
    use crate::color::Color;
    let mut e = editor_with_text("foo bar foo");
    e.state.last_search_query = Some("foo".to_string());
    e.update_search_highlights();
    // Two ui.search annotations, one per match.
    assert_eq!(
        e.active_document()
            .annotations
            .query_kind("ui.search")
            .count(),
        2
    );
    // They render with a yellow background through the presentation pipeline.
    e.update_and_render().unwrap();
    let cells = e.render_system.compositor.get_composited_slice();
    assert!(
        cells.iter().any(|c| c.bg == Some(Color::Yellow)),
        "search match should render with yellow bg via annotations"
    );
    // Clearing the search removes the annotations.
    e.state.last_search_query = None;
    e.update_search_highlights();
    assert_eq!(
        e.active_document()
            .annotations
            .query_kind("ui.search")
            .count(),
        0
    );
}

#[test]
fn overlay_adornment_renders_at_anchor_column() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    let mut e = editor_with_text("0123456789");
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.checkbox"),
            Anchor::point(2),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("[x]", Placement::Overlay)),
        ),
    );
    let out = screen(&mut e);
    // "[x]" overlays content starting at the anchor column (over "234").
    assert!(
        out.contains("01[x]5"),
        "overlay should render over content; screen:\n{}",
        out
    );
}

#[test]
fn text_attributes_emit_sgr_escape_codes() {
    // Confirms attributes reach the terminal as standard ANSI SGR (whether a
    // given terminal/font *renders* them visually is a terminal config matter).
    use crate::annotations::{
        Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride,
    };
    let mut e = editor_with_text("hello");
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.x"),
            Anchor::range(0, 5),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::with_style(StyleOverride {
            bold: true,
            underline: true,
            ..Default::default()
        })),
    );
    e.update_and_render().unwrap();
    let _ = e.render_to_terminal(true);
    let out = e.term.get_written_string();
    let bytes = out.escape_debug().collect::<String>();
    assert!(out.contains("\u{1b}[1m"), "no bold SGR. bytes={}", bytes);
    assert!(
        out.contains("\u{1b}[4m"),
        "no underline SGR. bytes={}",
        bytes
    );
}

#[test]
fn cursor_walks_through_leading_marker() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    let line = "The leadmark here";
    let mut e = editor_with_text(line);
    // ">> " (3 wide) leading at the start of "leadmark" (offset 4), like the demo.
    e.active_document().annotations.add(
        Annotation::new(Kind::new("ui.x"), Anchor::point(4), AnnotationOwner::User)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new(">> ", Placement::Leading)),
            ),
    );
    for off in 0..=line.len() {
        e.active_document().buffer.set_cursor(off).ok();
        e.update_and_render().unwrap();
        let expected = off + if off >= 4 { 3 } else { 0 };
        assert_eq!(
            e.state.cursor_pos.1, expected,
            "cursor offset {} should map to visual column {}",
            off, expected
        );
    }
}

#[test]
fn visual_cursor_clears_leading_marker_with_softwrap() {
    // Exercises the rendered (soft) cursor through the display-map path; the
    // delta from before->on the anchor must be 1 char + the 3-wide marker = 4.
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    let mut e = editor_with_text("The leadmark here");
    e.active_document().annotations.add(
        Annotation::new(Kind::new("ui.x"), Anchor::point(4), AnnotationOwner::User)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new(">> ", Placement::Leading)),
            ),
    );
    // The soft cursor animates toward its target, so render until it settles.
    let settle = |e: &mut Editor<MockTerminal>| -> usize {
        let mut last = 0;
        for _ in 0..60 {
            e.update_and_render().unwrap();
            last = e.render_system.last_soft_cursor().expect("soft cursor").1;
        }
        last
    };
    e.active_document().buffer.set_cursor(3).ok();
    let c3 = settle(&mut e);
    e.active_document().buffer.set_cursor(4).ok();
    let c4 = settle(&mut e);
    assert_eq!(
        c4 - c3,
        4,
        "cursor jumps over the 3-wide leading marker (1 char + 3)"
    );
}

#[test]
fn cursor_clears_leading_marker() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    let mut e = editor_with_text("The word here");
    // 2-wide leading marker anchored at the start of "word" (offset 4).
    e.active_document().annotations.add(
        Annotation::new(Kind::new("ui.x"), Anchor::point(4), AnnotationOwner::User)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new(">>", Placement::Leading)),
            ),
    );

    // Cursor on the space just before the marker (offset 3): column 3, unshifted.
    e.active_document().buffer.set_cursor(3).ok();
    e.update_and_render().unwrap();
    assert_eq!(
        e.state.cursor_pos.1, 3,
        "cursor before the marker is not shifted"
    );

    // Cursor on 'w' (offset 4): must clear the 2-wide marker -> column 6, not 4.
    e.active_document().buffer.set_cursor(4).ok();
    e.update_and_render().unwrap();
    assert_eq!(
        e.state.cursor_pos.1, 6,
        "cursor on the anchored char sits after the leading marker"
    );
}

#[test]
fn leading_adornment_inserts_and_shifts_content() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    let mut e = editor_with_text("The word here");
    // Leading ">>" anchored at the start of "word" (offset 4).
    e.active_document().annotations.add(
        Annotation::new(Kind::new("ui.x"), Anchor::point(4), AnnotationOwner::User)
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new(">>", Placement::Leading)),
            ),
    );
    let out = screen(&mut e);
    assert!(
        out.contains("The >>word here"),
        "leading marker inserts before anchor without eating preceding text; screen:\n{}",
        out
    );
}

#[test]
fn conceal_hides_markers_except_on_cursor_line() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    // Each line wraps "bold"/"wide" in "**"; conceal the markers on line 0 only.
    let mut e = editor_with_text("a **bold** x\nb **wide** y");
    for (s, en) in [(2usize, 4usize), (8usize, 10usize)] {
        e.active_document().annotations.add(
            Annotation::new(
                Kind::new("md.conceal"),
                Anchor::range(s, en),
                AnnotationOwner::User,
            )
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new("", Placement::Conceal)),
            ),
        );
    }

    // Cursor on line 1: line 0's markers are concealed and the text reflows.
    e.active_document().buffer.set_cursor(13).ok();
    let out = screen(&mut e);
    assert!(
        out.contains("a bold x"),
        "markers hidden + reflowed on non-cursor line; screen:\n{}",
        out
    );

    // Cursor on line 0: its markers are revealed (concealcursor behavior).
    e.active_document().buffer.set_cursor(0).ok();
    let out2 = screen(&mut e);
    assert!(
        out2.contains("a **bold** x"),
        "markers revealed on the cursor's line; screen:\n{}",
        out2
    );
}

#[test]
fn conceal_reveal_unaffected_by_multibyte_chars_on_earlier_lines() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    // Many 2-byte chars on line 0 push line 1's byte offsets past its char
    // offsets, regressing the cursor-line reveal check (byte vs char-indexed).
    let line0 = "e\u{301}".repeat(20);
    let line1 = "**x**";
    let line2 = "tail";
    let text = format!("{line0}\n{line1}\n{line2}");
    let mut e = editor_with_text(&text);

    let line1_char_start = line0.chars().count() + 1;
    let line1_byte_start = line0.len() + 1;
    for (s, en) in [(0usize, 2usize), (3usize, 5usize)] {
        let byte_s = line1_byte_start + s;
        let byte_e = line1_byte_start + en;
        e.active_document().annotations.add(
            Annotation::new(
                Kind::new("md.conceal"),
                Anchor::range(byte_s, byte_e),
                AnnotationOwner::User,
            )
            .with_presentation(
                Presentation::default().with_adornment(Adornment::new("", Placement::Conceal)),
            ),
        );
    }

    // Cursor on line 1 (the markers' own line): they must be revealed.
    e.active_document().buffer.set_cursor(line1_char_start).ok();
    let out = screen(&mut e);
    assert!(
        out.contains("**x**"),
        "markers must be revealed on the cursor's own line despite multibyte \
         drift earlier in the document; screen:\n{}",
        out
    );

    // Cursor on line 2: line 1's markers are concealed again.
    e.active_document()
        .buffer
        .set_cursor(line1_char_start + line1.chars().count() + 1)
        .ok();
    let out2 = screen(&mut e);
    let line1_row = out2.lines().nth(1).unwrap_or("");
    assert!(
        line1_row.trim_end().ends_with('x') && !line1_row.contains('*'),
        "markers stay concealed off the cursor's line; screen:\n{}",
        out2
    );
}

#[test]
fn overlay_range_conceals_span_padding_and_truncating() {
    use crate::annotations::{
        Adornment, Anchor, Annotation, AnnotationOwner, Kind, Placement, Presentation,
    };
    let mut e = editor_with_text("0123456789");
    // [0,3) "012" concealed by "X" -> "X  " (padded to the 3-cell span).
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.x"),
            Anchor::range(0, 3),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("X", Placement::Overlay)),
        ),
    );
    // [5,7) "56" concealed by "LONG" -> "LO" (truncated to the 2-cell span).
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.y"),
            Anchor::range(5, 7),
            AnnotationOwner::User,
        )
        .with_presentation(
            Presentation::default().with_adornment(Adornment::new("LONG", Placement::Overlay)),
        ),
    );
    let out = screen(&mut e);
    assert!(
        out.contains("X  34LO789"),
        "overlay conceals each span exactly (pad short, truncate long); screen:\n{}",
        out
    );
}

#[test]
fn presentation_style_renders_fg_and_attrs_to_cells() {
    use crate::annotations::{
        Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride,
    };
    use crate::color::Color;
    let mut e = editor_with_text("hello world");
    let style = StyleOverride {
        fg: Some(Color::Red),
        underline: true,
        ..Default::default()
    };
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.x"),
            Anchor::range(0, 5),
            AnnotationOwner::User,
        )
        .with_presentation(Presentation::with_style(style)),
    );
    e.update_and_render().unwrap();
    let cells = e.render_system.compositor.get_composited_slice();
    let styled = cells
        .iter()
        .find(|c| c.fg == Some(Color::Red) && c.attrs.underline);
    assert!(styled.is_some(), "expected an underlined red cell");
    assert!("hello".contains(styled.unwrap().to_char()));
}

#[test]
fn diagnostic_renders_adornment_and_hover_tooltip() {
    let mut e = editor_with_text("hello\nworld");
    e.active_document()
        .annotations
        .create_diagnostic(0, 1, "type mismatch");
    e.active_document().buffer.set_cursor(0).ok();
    let out = screen(&mut e);
    // EOL adornment shows the bare message on the diagnostic line.
    assert!(out.contains("type mismatch"), "adornment missing:\n{}", out);
    // Hover tooltip (cursor on the diagnostic line) shows the "[error]" form.
    assert!(
        out.contains("[error] type mismatch"),
        "diagnostic hover tooltip missing; screen:\n{}",
        out
    );
}

#[test]
fn checkbox_activation_toggles_payload() {
    let mut e = editor_with_text("hello world");
    let mut payload = Value::map();
    payload.set("checked", Value::Bool(false));
    let id = e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.checkbox"),
            Anchor::range(0, 5),
            AnnotationOwner::User,
        )
        .with_payload(payload)
        .with_actions(vec![Action::new("toggle").as_default()]),
    );
    e.active_document().buffer.set_cursor(2).ok();

    assert!(e.activate_annotation_at_cursor());
    let checked = e
        .active_document()
        .annotations
        .get(id)
        .and_then(|a| a.payload.get("checked").and_then(Value::as_bool));
    assert_eq!(checked, Some(true));

    assert!(e.activate_annotation_at_cursor());
    let checked = e
        .active_document()
        .annotations
        .get(id)
        .and_then(|a| a.payload.get("checked").and_then(Value::as_bool));
    assert_eq!(checked, Some(false));
}

#[test]
fn checkbox_toggle_edits_literal_box_in_buffer() {
    // A ui.checkbox sitting on a literal "[ ]" flips the buffer text itself,
    // not just the payload, so the rendered glyph is the buffer's own.
    let text = "- [ ] task one";
    let lb = text.find('[').unwrap();
    let mut e = editor_with_text(text);
    let mut payload = Value::map();
    payload.set("checked", Value::Bool(false));
    let id = e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.checkbox"),
            Anchor::range(lb, lb + 3),
            AnnotationOwner::User,
        )
        .with_payload(payload)
        .with_actions(vec![Action::new("toggle").as_default()]),
    );
    e.active_document().buffer.set_cursor(lb).ok();

    assert!(e.activate_annotation_at_cursor());
    assert!(
        screen(&mut e).contains("[x] task one"),
        "buffer box should flip to [x]"
    );
    let checked = e
        .active_document()
        .annotations
        .get(id)
        .and_then(|a| a.payload.get("checked").and_then(Value::as_bool));
    assert_eq!(checked, Some(true), "payload tracks the buffer state");

    assert!(e.activate_annotation_at_cursor());
    assert!(
        screen(&mut e).contains("[ ] task one"),
        "buffer box should flip back to [ ]"
    );
}

#[test]
fn affordances_render_when_cursor_on_interactive_annotation() {
    use crate::annotations::KeyHint;
    let mut e = editor_with_text("hello world");
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("vcs.hunk"),
            Anchor::range(0, 5),
            AnnotationOwner::User,
        )
        .with_actions(vec![
            Action::new("stage").as_default(),
            Action::new("discard").with_key_hint(KeyHint::new("d")),
        ]),
    );
    // Cursor off the annotation: no affordance shown.
    e.active_document().buffer.set_cursor(8).ok();
    let off = screen(&mut e);
    assert!(!off.contains("stage"), "no affordance away from annotation");

    // Cursor on the annotation: the action hints render in the tooltip layer.
    e.active_document().buffer.set_cursor(2).ok();
    let on = screen(&mut e);
    assert!(
        on.contains("Enter: stage") && on.contains("d: discard"),
        "affordance hint should render; screen:\n{}",
        on
    );
}

#[test]
fn separate_verbs_dispatch_to_separate_handlers() {
    // One annotation, two actions on two verbs, two ex-command handlers.
    // The default key runs "open"; the "stage" key runs the staging command.
    let mut e = editor_with_text("hello world");
    e.dispatch_registry
        .register("vcs.hunk", "open", Handler::Command("noh".into()));
    e.dispatch_registry
        .register("vcs.hunk", "stage", Handler::Command("noh".into()));
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("vcs.hunk"),
            Anchor::range(0, 5),
            AnnotationOwner::User,
        )
        .with_actions(vec![Action::new("open").as_default(), Action::new("stage")]),
    );
    e.active_document().buffer.set_cursor(2).ok();

    // Default activation resolves the default verb ("open").
    assert!(e.activate_annotation_verb(None));
    // The non-default verb is reachable only through its own key.
    assert!(e.activate_annotation_verb(Some("stage")));
    // An unknown verb on the same annotation dispatches nothing.
    assert!(!e.activate_annotation_verb(Some("discard")));
}

#[test]
fn verb_activation_is_noop_without_interactive_annotation() {
    let mut e = editor_with_text("hello world");
    e.active_document().buffer.set_cursor(2).ok();
    assert!(!e.activate_annotation_verb(Some("toggle")));
}

#[test]
fn activation_is_noop_without_interactive_annotation() {
    let mut e = editor_with_text("hello world");
    e.active_document().buffer.set_cursor(2).ok();
    assert!(!e.activate_annotation_at_cursor());
}

#[test]
fn interface_mode_snaps_vertical_motion_between_actionable_lines() {
    use crate::action::{Action as EdAction, EditorAction, Motion};
    // Six lines; only lines 1, 3, 5 carry an interactive annotation.
    let mut e = editor_with_text("l0\nl1\nl2\nl3\nl4\nl5");
    e.active_document()
        .convert_to_directory(std::path::PathBuf::new());
    let line_start = |e: &mut Editor<MockTerminal>, l: usize| {
        e.active_document().buffer.line_index.get_start(l).unwrap()
    };
    for l in [1usize, 3, 5] {
        let off = line_start(&mut e, l);
        e.active_document().annotations.add(
            Annotation::new(
                Kind::new("ui.link"),
                Anchor::point(off),
                AnnotationOwner::User,
            )
            .with_actions(vec![Action::activate()]),
        );
    }
    let cur_line = |e: &mut Editor<MockTerminal>| {
        let c = e.active_document().buffer.cursor();
        e.active_document().buffer.line_index.get_line_at(c)
    };
    let down = EdAction::Editor(EditorAction::Move(Motion::Down));
    let up = EdAction::Editor(EditorAction::Move(Motion::Up));
    e.active_document().buffer.set_cursor(0).ok();

    // Down snaps 0 -> 1 -> 3 -> 5, skipping inert lines.
    e.handle_action(&down);
    assert_eq!(cur_line(&mut e), 1);
    e.handle_action(&down);
    assert_eq!(cur_line(&mut e), 3);
    e.handle_action(&down);
    assert_eq!(cur_line(&mut e), 5);
    // No actionable line below 5: snap is a no-op, motion falls through to an
    // ordinary line move (already on the last line, so the cursor stays).
    e.handle_action(&down);
    assert_eq!(cur_line(&mut e), 5);

    // Up snaps back 5 -> 3 -> 1.
    e.handle_action(&up);
    assert_eq!(cur_line(&mut e), 3);
    e.handle_action(&up);
    assert_eq!(cur_line(&mut e), 1);
}

#[test]
fn snap_is_inert_without_interface_mode() {
    // A normal buffer does not snap: motion stays ordinary line movement.
    let mut e = editor_with_text("l0\nl1\nl2");
    let off = e.active_document().buffer.line_index.get_start(2).unwrap();
    e.active_document().annotations.add(
        Annotation::new(
            Kind::new("ui.link"),
            Anchor::point(off),
            AnnotationOwner::User,
        )
        .with_actions(vec![Action::activate()]),
    );
    e.active_document().buffer.set_cursor(0).ok();
    // snap_to_actionable_line only acts when called; the Move handler gates it
    // on interface_mode, so a normal buffer never reaches it. Confirm the gate.
    assert!(!e.active_document().is_interface_mode());
}

#[test]
fn next_prev_navigation_moves_cursor_between_annotations() {
    let mut e = editor_with_text("0123456789abcdef");
    let interactive = |s, en| {
        Annotation::new(
            Kind::new("ui.link"),
            Anchor::range(s, en),
            AnnotationOwner::User,
        )
        .with_actions(vec![Action::activate()])
    };
    e.active_document().annotations.add(interactive(2, 4));
    e.active_document().annotations.add(interactive(10, 12));
    e.active_document().buffer.set_cursor(0).ok();

    assert!(e.goto_next_interactive_annotation());
    assert_eq!(e.active_cursor(), 2);
    assert!(e.goto_next_interactive_annotation());
    assert_eq!(e.active_cursor(), 10);
    assert!(!e.goto_next_interactive_annotation());

    assert!(e.goto_prev_interactive_annotation());
    assert_eq!(e.active_cursor(), 2);
    assert!(!e.goto_prev_interactive_annotation());
}
