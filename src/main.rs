//! Rift - a terminal-based text editor. Main entry point: text mutation
//! flows through `Command` execution, so rendering is a pure read of state.
mod cli;

use monster_rift::editor::Editor;
use monster_rift::term::crossterm::CrosstermBackend;

use std::process::ExitCode;

fn main() -> ExitCode {
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
        eprintln!("{msg}");
    }));

    let args = match cli::parse() {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("rift v{}", cli::VERSION);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("rift: {e}");
            eprintln!("{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };

    // Internal mode: proxy one language server for editors to reattach to.
    #[cfg(feature = "lsp")]
    if let Some(key) = args.lsp_broker {
        return monster_rift::lsp::broker::run(&key);
    }

    #[cfg(feature = "ipc")]
    if args.list_sessions {
        return monster_rift::ipc::session::print_newest();
    }

    // daemon mode
    #[cfg(feature = "ipc")]
    if args.daemon {
        if args.detach {
            if let Err(e) = monster_rift::ipc::daemon::detach() {
                eprintln!("detach error: {e}");
                return ExitCode::FAILURE;
            }
            return ExitCode::SUCCESS;
        }
        if let Err(e) = monster_rift::ipc::daemon::run(args.file.as_deref()) {
            eprintln!("daemon error: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    // All interactive paths share one terminal backend.
    let backend = match CrosstermBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to create terminal backend: {e}");
            return ExitCode::FAILURE;
        }
    };

    // --connect [user@]host: SSH to find or start a session, then attach via tunnel.
    #[cfg(feature = "ipc")]
    if let Some(target) = args.connect {
        if let Err(e) =
            monster_rift::ipc::client::connect_remote(&target, args.file.as_deref(), backend)
        {
            eprintln!("connect error: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    // local editor
    let mut editor = match Editor::with_file(backend, args.file.as_deref()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Initialization Error [{}]: {}", e.code, e.message);
            return ExitCode::FAILURE;
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
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
