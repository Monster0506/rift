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
use rift::editor::Editor;
use rift::term::crossterm::CrosstermBackend;

fn main() {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let file_path = if args.len() > 1 {
        Some(args[1].clone())
    } else {
        None
    };

    // Create terminal backend
    let backend = match CrosstermBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to create terminal backend: {e}");
            std::process::exit(1);
        }
    };

    // Create editor with optional file
    let mut editor = match Editor::with_file(backend, file_path) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialize editor: {e}");
            std::process::exit(1);
        }
    };

    // Run editor
    if let Err(e) = editor.run() {
        eprintln!("Editor error: {e}");
        std::process::exit(1);
    }
}
