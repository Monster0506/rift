//! CLI argument parser for rift.
//!
//! Supported flags:
//!   -v / --version       Print version and exit
//!   +                    Jump to last line
//!   +N                   Jump to line N (1-indexed, last wins)
//!   +/pattern            Open at first match (last wins)
//!   -c cmd / --cmd cmd   Execute ex command after open (repeatable)
//!   [file]               File to open (at most one)

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
                "Usage: rift [+] [+N] [+/pattern] [-c cmd] [--cmd cmd] [-v|--version] [file]"
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
