use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[cfg(feature = "treesitter")]
fn generate_huge_rust_source() -> String {
    use std::fmt::Write as _;
    let mut src = String::with_capacity(4 * 1024 * 1024);
    for i in 0..8_000u32 {
        let _ = write!(
            src,
            "pub fn generated_fn_{i}(input: &str, count: usize) -> Result<String, std::io::Error> {{\n\
             \x20   let mut out = String::with_capacity(count.max(16));\n\
             \x20   for (idx, ch) in input.chars().enumerate() {{\n\
             \x20       if idx % 3 == 0 {{\n\
             \x20           out.push(ch.to_ascii_uppercase());\n\
             \x20       }} else {{\n\
             \x20           out.push(ch);\n\
             \x20       }}\n\
             \x20   }}\n\
             \x20   Ok(format!(\"{{}}-{{}}\", out, count))\n\
             }}\n\n",
        );
    }
    src
}

#[test]
#[ignore = "manual timing harness"]
#[cfg(feature = "treesitter")]
fn ttfp_open_huge_treesitter_annotation_doc() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner};
    use std::time::{Duration, Instant};

    let src = generate_huge_rust_source();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.rs");
    std::fs::write(&path, &src).unwrap();
    let path_str = path.to_string_lossy().into_owned();
    eprintln!(
        "doc: {} bytes, {} lines",
        src.len(),
        src.matches('\n').count()
    );

    let t = Instant::now();
    let empty = Editor::new(MockTerminal::new(50, 180)).unwrap();
    let fixed_init = t.elapsed();
    drop(empty);

    let t0 = Instant::now();
    let mut editor = Editor::with_file(MockTerminal::new(50, 180), Some(path_str.clone())).unwrap();
    let init = t0.elapsed();
    let ttfp = editor
        .startup_first_paint
        .map(|t| t.duration_since(t0))
        .unwrap_or(init);

    let t = Instant::now();
    {
        let doc = editor.active_document();
        let len = doc.buffer.len();
        let n = 10_000usize;
        for k in 0..n {
            let start = k * (len / n);
            let ann = Annotation::new(
                crate::annotations::Kind::new("diag.warning"),
                Anchor::range(start, (start + 12).min(len)),
                AnnotationOwner::Lsp,
            )
            .with_presentation(crate::annotations::Presentation::with_face(
                crate::annotations::FaceRef::new("diag.warning"),
            ));
            doc.annotations.add(ann);
        }
    }
    let annotate = t.elapsed();

    let t = Instant::now();
    editor.update_and_render().unwrap();
    let annotated_paint = t.elapsed();

    let t = Instant::now();
    let doc2 = crate::document::Document::from_file(9_999, &path_str).unwrap();
    let file_load = t.elapsed();
    drop(doc2);

    let t = Instant::now();
    let loaded = editor
        .language_loader
        .load_language_for_file(&path)
        .unwrap();
    let q_src = editor
        .language_loader
        .load_query(&loaded.name, "highlights")
        .unwrap();
    let query = tree_sitter::Query::new(&loaded.language, &q_src).unwrap();
    let syntax_setup = t.elapsed();
    drop(query);

    let t = Instant::now();
    editor.update_lua_state();
    let lua_snapshot = t.elapsed();

    let t = Instant::now();
    let buf_clone = editor.active_document().buffer.clone();
    let buffer_clone = t.elapsed();
    drop(buf_clone);

    let t = Instant::now();
    let fresh_map = super::resolve_display_map(
        editor.document_manager.active_document().unwrap(),
        178,
        editor.state.settings.soft_wrap,
        editor.state.settings.wrap_width,
    );
    let map_build = t.elapsed();
    drop(fresh_map);

    let t = Instant::now();
    editor.force_full_redraw().unwrap();
    let full_redraw = t.elapsed();

    let t = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(30);
    while editor
        .active_document()
        .syntax
        .as_ref()
        .and_then(|s| s.tree.as_ref())
        .is_none()
    {
        if Instant::now() >= deadline {
            eprintln!("WARNING: background parse never landed");
            break;
        }
        if let Ok(msg) = editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(20))
        {
            editor.handle_job_message(msg).unwrap();
        }
    }
    let parse_wait = t.elapsed();
    let t = Instant::now();
    editor.update_and_render().unwrap();
    let highlighted_paint = t.elapsed();

    eprintln!("--- TTFP phases ---");
    eprintln!("TTFP (open to first paint):          {ttfp:>10.2?}");
    eprintln!("empty-doc Editor::new (fixed cost):  {fixed_init:>10.2?}");
    eprintln!("with_file (startup to input loop):   {init:>10.2?}");
    eprintln!("  file load + line index (re-run):   {file_load:>10.2?}");
    eprintln!("  grammar + query compile (re-run):  {syntax_setup:>10.2?}");
    eprintln!("  lua snapshot, one call (re-run):   {lua_snapshot:>10.2?}");
    eprintln!("  buffer clone for parse job (rr):   {buffer_clone:>10.2?}");
    eprintln!("  display map build (re-run):        {map_build:>10.2?}");
    eprintln!("  full-frame redraw (steady state):  {full_redraw:>10.2?}");
    eprintln!("add 10k annotations (harness only):  {annotate:>10.2?}");
    eprintln!("annotated repaint:                   {annotated_paint:>10.2?}");
    eprintln!("background parse wait:               {parse_wait:>10.2?}");
    eprintln!("highlighted repaint:                 {highlighted_paint:>10.2?}");
}

#[cfg(feature = "treesitter")]
fn generate_prose_markdown() -> String {
    use std::fmt::Write as _;
    let mut src = String::with_capacity(1_300_000);
    let sentence = "Call me Ishmael. Some years ago, never mind how long precisely, \
        having little or no money in my purse, and nothing particular to interest me \
        on shore, I thought I would sail about a little and see the watery part of the world. ";
    for i in 0..1_300 {
        let _ = writeln!(src, "## CHAPTER FRAGMENT {i}\n");
        let _ = writeln!(src, "{}\n", sentence.repeat(2 + (i % 5)));
    }
    src
}

#[cfg(feature = "treesitter")]
fn measure_scroll(
    editor: &mut Editor<MockTerminal>,
    label: &str,
    motion: crate::action::Motion,
    n: usize,
) {
    use std::time::Instant;
    let mut times = Vec::with_capacity(n);
    let mut bytes = Vec::with_capacity(n);
    let mut scrolls = 0usize;
    let mut rides = 0usize;
    let mut top_changes = 0usize;
    for _ in 0..n {
        editor.term.clear();
        let top_before = editor.render_system.viewport.top_visual_row();
        let t = Instant::now();
        editor.execute_buffer_command(crate::command::Command::Move(motion, 1));
        editor.update_and_render().unwrap();
        times.push(t.elapsed());
        if editor.render_system.viewport.top_visual_row() != top_before {
            top_changes += 1;
        }
        let written = editor.term.get_written_string();
        bytes.push(written.len());
        if written.len() > 500 {
            scrolls += 1;
        }
        if written.contains("\u{1b}[1S") || written.contains("\u{1b}[1T") {
            rides += 1;
        }
    }
    times.sort();
    bytes.sort();
    let avg = times.iter().sum::<std::time::Duration>() / n as u32;
    eprintln!(
        "{label:<16} avg={avg:>9.2?} p95={:>9.2?} max={:>9.2?} | bytes p95={} max={} | top_changes={top_changes} scrolls={scrolls} rides={rides}",
        times[n * 95 / 100],
        times[n - 1],
        bytes[n * 95 / 100],
        bytes[n - 1]
    );
}

#[test]
#[ignore = "manual timing harness"]
#[cfg(feature = "treesitter")]
fn scroll_latency_wrapped_markdown() {
    use std::time::{Duration, Instant};

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let real = std::path::PathBuf::from(&home)
        .join("Documents")
        .join("moby_dick.md");
    let src = if real.exists() {
        eprintln!("using {}", real.display());
        std::fs::read_to_string(&real).unwrap()
    } else {
        eprintln!("using synthetic prose");
        generate_prose_markdown()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prose.md");
    std::fs::write(&path, &src).unwrap();
    eprintln!(
        "doc: {} bytes, {} lines",
        src.len(),
        src.matches('\n').count()
    );

    let mut editor = Editor::with_file(
        MockTerminal::new(50, 180),
        Some(path.to_string_lossy().into_owned()),
    )
    .unwrap();

    let deadline = Instant::now() + Duration::from_secs(30);
    while editor
        .active_document()
        .syntax
        .as_ref()
        .and_then(|s| s.tree.as_ref())
        .is_none()
    {
        if Instant::now() >= deadline {
            eprintln!("WARNING: background parse never landed");
            break;
        }
        if let Ok(msg) = editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(20))
        {
            editor.handle_job_message(msg).unwrap();
        }
    }
    editor.update_and_render().unwrap();

    eprintln!("--- scroll latency (soft wrap on, one j per sample) ---");
    {
        let cursor = editor.active_document().buffer.cursor();
        let line = editor.active_document().buffer.get_line();
        eprintln!(
            "pre: cursor={cursor} line={line} top_vr={} soft_wrap={}",
            editor.render_system.viewport.top_visual_row(),
            editor.state.settings.soft_wrap
        );
    }
    measure_scroll(&mut editor, "j from top", crate::action::Motion::Down, 300);
    {
        let cursor = editor.active_document().buffer.cursor();
        let line = editor.active_document().buffer.get_line();
        eprintln!(
            "post: cursor={cursor} line={line} top_vr={}",
            editor.render_system.viewport.top_visual_row()
        );
    }

    editor.goto_line(2_500);
    editor.update_and_render().unwrap();
    measure_scroll(
        &mut editor,
        "j from middle",
        crate::action::Motion::Down,
        300,
    );
    measure_scroll(&mut editor, "k back up", crate::action::Motion::Up, 300);

    editor.state.last_search_query = Some("the".to_string());
    editor.update_search_highlights();
    editor.update_and_render().unwrap();
    eprintln!("search matches: {}", editor.state.search_matches.len());
    measure_scroll(&mut editor, "j with /the", crate::action::Motion::Down, 300);

    editor.goto_line(0);
    editor.update_and_render().unwrap();
    measure_scroll(&mut editor, "j near end", crate::action::Motion::Down, 300);

    editor.goto_line(2_500);
    editor.current_mode = crate::mode::Mode::Insert;
    measure_type(&mut editor, "type with /the", 200);
    editor.current_mode = crate::mode::Mode::Normal;
    editor.state.last_search_query = None;
    editor.update_search_highlights();
    editor.update_and_render().unwrap();
    editor.current_mode = crate::mode::Mode::Insert;
    measure_type(&mut editor, "type no search", 200);

    {
        let t = Instant::now();
        editor.execute_buffer_command(crate::command::Command::InsertChar('y'));
        let exec = t.elapsed();
        let t = Instant::now();
        editor.update_and_render().unwrap();
        let render = t.elapsed();
        let t = Instant::now();
        editor.execute_buffer_command(crate::command::Command::InsertChar('y'));
        let exec2 = t.elapsed();
        let t = Instant::now();
        editor.update_lua_state();
        let lua_rebuild = t.elapsed();
        let t = Instant::now();
        editor.update_lua_state();
        let lua_hit = t.elapsed();
        let ann_count = editor.active_document().annotations.iter().count();
        eprintln!(
            "typing split: exec={exec:.2?}/{exec2:.2?} render={render:.2?} lua_rebuild={lua_rebuild:.2?} lua_hit={lua_hit:.2?} annotations={ann_count}"
        );
    }
}
