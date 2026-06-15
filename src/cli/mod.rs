//! CLI argument parser for rift.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where to position the cursor after opening.
#[derive(Debug, PartialEq)]
pub enum Goto {
    LastLine,
    Line(usize),
}

/// Parsed CLI arguments.
#[derive(Debug, Default, PartialEq)]
pub struct Args {
    pub file: Option<String>,
    pub goto: Option<Goto>,
    pub search: Option<String>,
    pub commands: Vec<String>,
    pub daemon: bool,
    /// Run daemon in background (detach from terminal).
    pub detach: bool,
    pub attach: Option<String>,
    /// SSH connect: [user@]host -- find newest remote session and attach.
    pub connect: Option<String>,
    pub bind: String,
    pub port: u16,
    /// Print newest live session as JSON and exit (used by --connect over SSH).
    pub list_sessions: bool,
    /// Start a daemon on the remote before attaching (used with --connect).
    pub start: bool,
}

impl Args {
    pub fn new() -> Self {
        Self {
            bind: "127.0.0.1".into(),
            port: 7619,
            ..Default::default()
        }
    }
}

/// Parse `std::env::args()`. Prints version and exits 0 for `-v`/`--version`.
/// Prints usage and exits 1 on any error.
pub fn parse() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let strs: Vec<&str> = raw.iter().map(String::as_str).collect();
    match parse_args(&strs) {
        Ok(None) => {
            println!("rift v{VERSION}");
            std::process::exit(0);
        }
        Ok(Some(args)) => args,
        Err(e) => {
            eprintln!("rift: {e}");
            eprintln!(
                "Usage: rift [+] [+N] [+/pattern] [-c cmd] [--cmd cmd] [-v|--version] [--daemon] [-d|--detach] [--attach <file>] [--connect [user@]host] [--bind <addr>] [--port <n>] [file]"
            );
            std::process::exit(1);
        }
    }
}

/// Internal parser — returns:
///   `Ok(None)`       → version flag seen, caller should print version + exit
///   `Ok(Some(args))` → success
///   `Err(msg)`       → bad input, caller should print error + exit
pub fn parse_args(args: &[&str]) -> Result<Option<Args>, String> {
    let mut result = Args::new();
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
        } else if arg == "--attach" {
            // Optional path: --attach [file]
            // If next token is absent or looks like a flag, auto-discover.
            let next = args.get(i + 1);
            let is_path = next
                .map(|s| !s.starts_with('-') && !s.is_empty())
                .unwrap_or(false);
            if is_path {
                i += 1;
                result.attach = Some(args[i].to_string());
            } else {
                result.attach = Some(String::new()); // empty sentinel = auto-discover
            }
        } else if arg == "--bind" {
            i += 1;
            match args.get(i) {
                Some(addr) => result.bind = addr.to_string(),
                None => return Err("'--bind' requires an address".into()),
            }
        } else if arg == "--port" {
            i += 1;
            match args.get(i) {
                Some(p) => match p.parse::<u16>() {
                    Ok(n) => result.port = n,
                    Err(_) => return Err(format!("invalid port number: '{p}'")),
                },
                None => return Err("'--port' requires a port number".into()),
            }
        } else if arg == "--list-sessions" {
            result.list_sessions = true;
        } else if arg == "--start" {
            result.start = true;
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
