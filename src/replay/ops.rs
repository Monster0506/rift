//! Parser for the replay script DSL (open/size/keys/wait/mark/assert) used
//! to drive a scripted `Editor` session for profiling and regression checks.

use crate::key::{parse_key_sequence, Key};

/// A single check against editor state at the point it appears in a script.
#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
    Cursor { row: usize, col: usize },
    Mode(String),
    Line { row: usize, text: String },
    Buffer(String),
}

/// One step of a parsed replay script.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptOp {
    /// Open a file before the session starts.
    Open(String),
    /// Start with an empty buffer instead of `Open`.
    New,
    /// Terminal dimensions for the session.
    Size { rows: u16, cols: u16 },
    /// Feed a vim-notation key sequence, one `tick()` per key.
    Keys(Vec<Key>),
    /// Tick until job/LSP/plugin work is quiet, or the timeout elapses.
    WaitIdle { timeout_ms: u64 },
    /// Record a named timing checkpoint.
    Mark(String),
    /// Check editor state, aborting the script on mismatch.
    Assert(Assertion),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

const DEFAULT_WAIT_IDLE_MS: u64 = 2000;

/// Parse a replay script into a sequence of [`ScriptOp`]s.
pub fn parse(source: &str) -> Result<Vec<ScriptOp>, ParseError> {
    let mut ops = Vec::new();
    let mut lines = source.lines().enumerate().peekable();

    while let Some((idx, raw)) = lines.next() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (verb, rest) = split_verb(line);
        let op = match verb {
            "open" => ScriptOp::Open(rest.to_string()),
            "new" => ScriptOp::New,
            "size" => {
                let (rows, cols) = parse_two_usize(rest, line_no, "size")?;
                ScriptOp::Size {
                    rows: rows as u16,
                    cols: cols as u16,
                }
            }
            "keys" => {
                let keys = parse_key_sequence(rest).ok_or_else(|| ParseError {
                    line: line_no,
                    message: format!("invalid key sequence: {rest}"),
                })?;
                ScriptOp::Keys(keys)
            }
            "wait" => parse_wait(rest, line_no)?,
            "mark" => ScriptOp::Mark(rest.to_string()),
            "assert" => parse_assert(rest, &mut lines, line_no)?,
            other => {
                return Err(ParseError {
                    line: line_no,
                    message: format!("unknown directive: {other}"),
                })
            }
        };
        ops.push(op);
    }

    Ok(ops)
}

fn split_verb(line: &str) -> (&str, &str) {
    match line.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (line, ""),
    }
}

fn parse_two_usize(rest: &str, line_no: usize, ctx: &str) -> Result<(usize, usize), ParseError> {
    let mut parts = rest.split_whitespace();
    let a = parts.next().and_then(|s| s.parse().ok());
    let b = parts.next().and_then(|s| s.parse().ok());
    match (a, b) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(ParseError {
            line: line_no,
            message: format!("{ctx} requires two numbers, got: {rest}"),
        }),
    }
}

fn parse_wait(rest: &str, line_no: usize) -> Result<ScriptOp, ParseError> {
    let mut parts = rest.split_whitespace();
    match parts.next() {
        Some("idle") => {
            let timeout_ms = match parts.next() {
                Some(ms) => ms.parse().map_err(|_| ParseError {
                    line: line_no,
                    message: format!("invalid timeout: {ms}"),
                })?,
                None => DEFAULT_WAIT_IDLE_MS,
            };
            Ok(ScriptOp::WaitIdle { timeout_ms })
        }
        _ => Err(ParseError {
            line: line_no,
            message: format!("unknown wait kind: {rest}"),
        }),
    }
}

fn parse_assert<'a>(
    rest: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    line_no: usize,
) -> Result<ScriptOp, ParseError> {
    let (kind, args) = split_verb(rest);
    let assertion = match kind {
        "cursor" => {
            let (row, col) = parse_two_usize(args, line_no, "assert cursor")?;
            Assertion::Cursor { row, col }
        }
        "mode" => Assertion::Mode(args.to_string()),
        "line" => {
            let (row_str, text) = split_verb(args);
            let row = row_str.parse().map_err(|_| ParseError {
                line: line_no,
                message: format!("invalid line number: {row_str}"),
            })?;
            Assertion::Line {
                row,
                text: unquote(text),
            }
        }
        "buffer" => Assertion::Buffer(read_fenced_block(lines, line_no)?),
        other => {
            return Err(ParseError {
                line: line_no,
                message: format!("unknown assert kind: {other}"),
            })
        }
    };
    Ok(ScriptOp::Assert(assertion))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn read_fenced_block<'a>(
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    start_line: usize,
) -> Result<String, ParseError> {
    match lines.next() {
        Some((_, l)) if l.trim() == "<<<" => {}
        _ => {
            return Err(ParseError {
                line: start_line,
                message: "expected '<<<' to open a block".to_string(),
            })
        }
    }
    let mut body = Vec::new();
    loop {
        match lines.next() {
            Some((_, l)) if l.trim() == ">>>" => break,
            Some((_, l)) => body.push(l.to_string()),
            None => {
                return Err(ParseError {
                    line: start_line,
                    message: "unterminated block, expected '>>>'".to_string(),
                })
            }
        }
    }
    Ok(body.join("\n"))
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
