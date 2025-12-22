//! Rift - A terminal-based text editor
//! Main entry point

use rift::editor::Editor;
use rift::term::crossterm::CrosstermBackend;

fn main() {
    // Create terminal backend
    let backend = match CrosstermBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to create terminal backend: {}", e);
            std::process::exit(1);
        }
    };

    // Create editor
    let mut editor = match Editor::new(backend) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialize editor: {}", e);
            std::process::exit(1);
        }
    };

    // Run editor
    if let Err(e) = editor.run() {
        eprintln!("Editor error: {}", e);
        std::process::exit(1);
    }
}
