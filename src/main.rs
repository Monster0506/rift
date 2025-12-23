//! Rift - A terminal-based text editor
//! Main entry point
//! # Rift Invariants
//!
//! These invariants define the architectural and semantic guarantees of Rift.
//! They are non-negotiable unless the design is intentionally revised.
//!
//! Breaking an invariant to fix a bug indicates a design error, not an
//! implementation shortcut.

/// ## Global Invariants
///
/// - The editor core never depends on terminal implementation details.
/// - All mutations of text flow through `Command` execution.
/// - Rendering is a pure read of editor state and buffer contents.
/// - Input handling never mutates editor state directly.
/// - Panics or early exits always restore terminal state.
/// - Editor behavior is deterministic for a given sequence of commands.

/// ## buffer/ Invariants
///
/// - The buffer owns both text storage and cursor position.
/// - The cursor is always located at the gap.
/// - `gap_start <= gap_end` at all times.
/// - All text before the gap is logically before the cursor.
/// - All text after the gap is logically after the cursor.
/// - Buffer contents are treated consistently as either UTF-8 or raw bytes.
/// - Movement operations never mutate text.
/// - Insert and delete operations never leave the buffer in an invalid state.
/// - Buffer methods either succeed fully or perform no mutation.
/// - The buffer never emits or interprets commands.

/// ## command/ Invariants
///
/// - `Command` represents editor-level intent, not key-level input.
/// - Commands contain no terminal- or platform-specific concepts.
/// - All data required to apply a command is contained within the command.
/// - Commands are immutable once created.
/// - Adding a new command requires explicit executor support.

/// ## executor/ Invariants
///
/// - The executor mutates buffer and editor state only.
/// - Each command application is atomic.
/// - Mode changes are not handled here unless explicitly documented.
/// - Executor behavior is independent of key bindings.
/// - Executor never inspects raw input or terminal state.
/// - Commands are applied strictly in sequence.

/// ## key_handler/ Invariants
///
/// - Key handlers translate input events into `Command`s.
/// - Key handlers never mutate buffer or editor state directly.
/// - Key handlers are mode-aware but buffer-agnostic.
/// - Multi-key sequences are handled entirely within this layer.
/// - Invalid or incomplete sequences yield `Noop` or deferred input.
/// - Key handling is deterministic.

/// ## state/ Invariants
///
/// - Editor mode is explicit and globally consistent.
/// - State transitions occur only through well-defined control flow.
/// - There is exactly one active buffer at a time in v0.
/// - Editor state is never partially updated.
/// - State changes are observable by the renderer but never influenced by it.

/// ## viewport/ Invariants
///
/// - The viewport represents a window into buffer content.
/// - The viewport never mutates buffer contents.
/// - The cursor is always visible within the viewport.
/// - Viewport dimensions reflect the current terminal size.
/// - Viewport updates are explicit and predictable.
/// - Viewport logic is independent of rendering mechanics.

/// ## render/ Invariants
///
/// - Rendering reads editor state and buffer contents only.
/// - Rendering never mutates editor, buffer, or cursor state.
/// - Rendering performs no input handling.
/// - Rendering tolerates invalid state but never corrects it.
/// - Displayed cursor position always matches buffer cursor position.
/// - A full redraw is always safe.

/// ## status/ Invariants
///
/// - Status content is derived entirely from editor state.
/// - Status rendering does not influence editor behavior.
/// - Status display is optional and failure-tolerant.
/// - Status never consumes input or commands.

/// ## term/ Invariants
///
/// - Terminal handling is isolated behind a strict abstraction boundary.
/// - Raw mode is enabled before input processing begins.
/// - Terminal state is restored on normal exit and on panic.
/// - Terminal size queries are accurate at the time of use.
/// - Terminal code never depends on editor internals.

/// ## test_utils/ Invariants
///
/// - Test utilities introduce no production-only behavior.
/// - Tests assert invariants, not implementation details.
/// - Buffer and executor logic are testable without a terminal.
/// - Boundary and edge cases are explicitly tested.

/// ## Meta-Invariant
///
/// - If fixing a bug requires breaking an invariant, the design is wrong.


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
