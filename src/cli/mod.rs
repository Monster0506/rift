//! CLI argument parser for rift.

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Goto {
    LastLine,
    Line(usize),
}

/// Parsed CLI arguments.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Args {
    pub(crate) file: Option<String>,
    pub(crate) goto: Option<Goto>,
    pub(crate) search: Option<String>,
    pub(crate) commands: Vec<String>,
    pub(crate) daemon: bool,
    pub(crate) detach: bool,
    pub(crate) connect: Option<String>,
    pub(crate) list_sessions: bool,
    pub(crate) lsp_broker: Option<String>,
}

pub(crate) const USAGE: &str = "Usage: rift [+] [+N] [+/pattern] [-c cmd] [--cmd cmd] [-v|--version] [--daemon] [-d|--detach] [--connect [user@]host] [--list-sessions] [file]";

pub(crate) fn parse() -> Result<Option<Args>, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let strs: Vec<&str> = raw.iter().map(String::as_str).collect();
    parse_args(&strs)
}

pub(crate) fn parse_args(args: &[&str]) -> Result<Option<Args>, String> {
    let mut result = Args::default();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i];

        if arg == "-v" || arg == "--version" {
            return Ok(None);
        } else if arg == "+" {
            result.goto = Some(Goto::LastLine);
        } else if let Some(pattern) = arg.strip_prefix("+/") {
            result.search = Some(pattern.to_string());
        } else if let Some(rest) = arg.strip_prefix('+') {
            match rest.parse::<usize>() {
                Ok(n) => result.goto = Some(Goto::Line(n)),
                Err(_) => return Err(format!("invalid line number: '{rest}'")),
            }
        } else if arg == "-c" || arg == "--cmd" {
            i += 1;
            match args.get(i) {
                Some(cmd) => result.commands.push(cmd.to_string()),
                None => return Err(format!("'{arg}' requires a command argument")),
            }
        } else if arg == "--daemon" {
            result.daemon = true;
        } else if arg == "--detach" || arg == "-d" {
            result.detach = true;
        } else if arg == "--connect" {
            i += 1;
            match args.get(i) {
                Some(target) => result.connect = Some(target.to_string()),
                None => return Err("'--connect' requires a [user@]host argument".into()),
            }
        } else if arg == "--list-sessions" {
            result.list_sessions = true;
        } else if arg == "--lsp-broker" {
            i += 1;
            match args.get(i) {
                Some(key) => result.lsp_broker = Some(key.to_string()),
                None => return Err("'--lsp-broker' requires a key argument".into()),
            }
        } else if arg.starts_with('-') {
            return Err(format!("unknown flag: '{arg}'"));
        } else {
            if result.file.is_some() {
                return Err(format!(
                    "unexpected argument: '{arg}' (only one file is supported)"
                ));
            }
            result.file = Some(arg.to_string());
        }

        i += 1;
    }

    Ok(Some(result))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
