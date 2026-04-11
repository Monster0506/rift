//! Rift - A terminal-based text editor
//! Main entry point
//! # Rift Invariants
//!
//! These invariants define the architectural and semantic guarantees of Rift.
//! They are non-negotiable unless the design is intentionally revised.
//!
//! Breaking an invariant to fix a bug indicates a design error, not an
//! implementation shortcut.
/// ## main/ Invariants
///
/// - The editor core never depends on terminal implementation details.
/// - All mutations of text flow through `Command` execution.
/// - Rendering is a pure read of editor state and buffer contents.
/// - Input handling never mutates editor state directly.
/// - Panics or early exits always restore terminal state.
/// - Editor behavior is deterministic for a given sequence of commands.
mod cli;

use monster_rift::editor::Editor;
use monster_rift::term::crossterm::CrosstermBackend;

fn main() {
    // Raw mode swallows stderr, so log panics to a file.
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let msg = format!(
            "RIFT PANIC: {}\n  at {:?}\n\nBacktrace:\n{}\n",
            info,
            info.location(),
            backtrace,
        );
        let _ = std::fs::write(std::env::temp_dir().join("rift-panic.log"), &msg);
        let _ = std::fs::write("rift-panic.log", &msg);
        eprintln!("{}", msg);
    }));

    let args = cli::parse();

    // Create terminal backend
    let backend = match CrosstermBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to create terminal backend: {e}");
            std::process::exit(1);
        }
    };

    // Create editor with optional file
    let mut editor = match Editor::with_file(backend, args.file) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Initialization Error [{}]: {}", e.code, e.message);
            std::process::exit(1);
        }
    };

    // Apply startup commands before first render
    for cmd in args.commands {
        editor.run_command(cmd);
    }

    // Apply cursor position
    if let Some(goto) = args.goto {
        match goto {
            cli::Goto::LastLine => editor.goto_line(0),
            cli::Goto::Line(n) => editor.goto_line(n),
        }
    }

    // Apply search
    if let Some(pattern) = args.search {
        editor.jump_to_pattern(&pattern);
    }

    // Run editor
    if let Err(e) = editor.run() {
        eprintln!("Editor Error [{}]: {}", e.code, e.message);
        std::process::exit(1);
    }
}
