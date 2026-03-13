//! Integer expression evaluator for settings values.
//!
//! Supports standard arithmetic (`+`, `-`, `*`, `/`) with correct precedence,
//! parentheses, integer literals, and caller-supplied keyword substitution.
//!
//! Keywords are resolved via a closure at evaluation time, so dynamic values
//! like `auto` (terminal width) can be substituted without storing the width
//! in the setting itself.
//!
//! # Example
//! ```rust,ignore
//! let width = eval("auto / 2 + 5", &|kw| if kw == "auto" { Some(120) } else { None });
//! assert_eq!(width, Ok(65));
//! ```

pub fn eval(input: &str, lookup: &dyn Fn(&str) -> Option<usize>) -> Result<usize, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let v = parse_expr(&tokens, &mut pos, lookup)?;
    if pos != tokens.len() {
        return Err(format!("unexpected token at position {pos}"));
    }
    Ok(v)
}

#[derive(Debug)]
enum Tok {
    Num(usize),
    Word(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(&c) = it.peek() {
        match c {
            ' ' | '\t' => {
                it.next();
            }
            '0'..='9' => {
                let mut n: usize = 0;
                while let Some(&d @ '0'..='9') = it.peek() {
                    n = n.saturating_mul(10).saturating_add(d as usize - '0' as usize);
                    it.next();
                }
                out.push(Tok::Num(n));
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut w = String::new();
                while let Some(&c) = it.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        w.push(c);
                        it.next();
                    } else {
                        break;
                    }
                }
                out.push(Tok::Word(w));
            }
            '+' => {
                out.push(Tok::Plus);
                it.next();
            }
            '-' => {
                out.push(Tok::Minus);
                it.next();
            }
            '*' => {
                out.push(Tok::Star);
                it.next();
            }
            '/' => {
                out.push(Tok::Slash);
                it.next();
            }
            '(' => {
                out.push(Tok::LParen);
                it.next();
            }
            ')' => {
                out.push(Tok::RParen);
                it.next();
            }
            _ => return Err(format!("unexpected character '{c}'")),
        }
    }
    Ok(out)
}

fn parse_expr(
    toks: &[Tok],
    pos: &mut usize,
    lookup: &dyn Fn(&str) -> Option<usize>,
) -> Result<usize, String> {
    let mut v = parse_term(toks, pos, lookup)?;
    while *pos < toks.len() {
        match toks[*pos] {
            Tok::Plus => {
                *pos += 1;
                v = v.saturating_add(parse_term(toks, pos, lookup)?);
            }
            Tok::Minus => {
                *pos += 1;
                v = v.saturating_sub(parse_term(toks, pos, lookup)?);
            }
            _ => break,
        }
    }
    Ok(v)
}

fn parse_term(
    toks: &[Tok],
    pos: &mut usize,
    lookup: &dyn Fn(&str) -> Option<usize>,
) -> Result<usize, String> {
    let mut v = parse_atom(toks, pos, lookup)?;
    while *pos < toks.len() {
        match toks[*pos] {
            Tok::Star => {
                *pos += 1;
                v = v.saturating_mul(parse_atom(toks, pos, lookup)?);
            }
            Tok::Slash => {
                *pos += 1;
                let rhs = parse_atom(toks, pos, lookup)?;
                if rhs == 0 {
                    return Err("division by zero".to_string());
                }
                v /= rhs;
            }
            _ => break,
        }
    }
    Ok(v)
}

fn parse_atom(
    toks: &[Tok],
    pos: &mut usize,
    lookup: &dyn Fn(&str) -> Option<usize>,
) -> Result<usize, String> {
    if *pos >= toks.len() {
        return Err("unexpected end of expression".to_string());
    }
    match &toks[*pos] {
        Tok::Num(n) => {
            let v = *n;
            *pos += 1;
            Ok(v)
        }
        Tok::Word(w) => {
            let key = w.to_lowercase();
            match lookup(&key) {
                Some(v) => {
                    *pos += 1;
                    Ok(v)
                }
                None => Err(format!("unknown keyword '{w}'")),
            }
        }
        Tok::LParen => {
            *pos += 1;
            let v = parse_expr(toks, pos, lookup)?;
            if !matches!(toks.get(*pos), Some(Tok::RParen)) {
                return Err("expected ')'".to_string());
            }
            *pos += 1;
            Ok(v)
        }
        tok => Err(format!("unexpected token {tok:?}")),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
