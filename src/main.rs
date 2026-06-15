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

    if args.list_sessions {
        monster_rift::ipc::session::print_newest();
    }

    // daemon mode
    if args.daemon {
        if args.detach {
            if let Err(e) = monster_rift::ipc::daemon::detach() {
                eprintln!("detach error: {e}");
                std::process::exit(1);
            }
            return;
        }
        let cfg = monster_rift::ipc::daemon::DaemonConfig {
            bind: args.bind,
            port: args.port,
        };
        if let Err(e) = monster_rift::ipc::daemon::run(cfg, args.file) {
            eprintln!("daemon error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // All interactive paths share one terminal backend.
    let backend = match CrosstermBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to create terminal backend: {e}");
            std::process::exit(1);
        }
    };

    // --connect [user@]host: SSH to find newest session, then attach.
    if let Some(target) = args.connect {
        if let Err(e) = monster_rift::ipc::client::connect_remote(
            &target, args.start, args.file, args.port, backend,
        ) {
            eprintln!("connect error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // --attach [file]: attach to a local daemon session.
    if let Some(session_arg) = args.attach {
        let session_path = if session_arg.is_empty() {
            match monster_rift::ipc::session::find_local() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("attach error: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            std::path::PathBuf::from(session_arg)
        };
        let cfg = monster_rift::ipc::client::AttachConfig {
            session_file: session_path,
            skip_liveness: false,
        };
        if let Err(e) = monster_rift::ipc::client::attach(cfg, backend) {
            eprintln!("attach error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // local editor
    let mut editor = match Editor::with_file(backend, args.file) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Initialization Error [{}]: {}", e.code, e.message);
            std::process::exit(1);
        }
    };

    for cmd in args.commands {
        editor.run_command(cmd);
    }
    if let Some(goto) = args.goto {
        match goto {
            cli::Goto::LastLine => editor.goto_line(0),
            cli::Goto::Line(n) => editor.goto_line(n),
        }
    }
    if let Some(pattern) = args.search {
        editor.jump_to_pattern(&pattern);
    }

    if let Err(e) = editor.run() {
        eprintln!("Editor Error [{}]: {}", e.code, e.message);
        std::process::exit(1);
    }
}
