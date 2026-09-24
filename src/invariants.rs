//! Shared types for internal-consistency checks that can run against live
//! editor state via [`crate::editor::Editor::assert_invariants`].

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InvariantTier {
    Fast,
    Standard,
    Deep,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub area: &'static str,
    pub detail: String,
}

impl Violation {
    pub fn new(area: &'static str, detail: impl Into<String>) -> Self {
        Self {
            area,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.area, self.detail)
    }
}

pub(crate) fn extend(out: &mut Vec<Violation>, area: &'static str, details: Vec<String>) {
    out.extend(
        details
            .into_iter()
            .map(|detail| Violation::new(area, detail)),
    );
}

#[cfg(debug_assertions)]
pub fn log_violations(violations: &[Violation]) {
    if violations.is_empty() {
        return;
    }
    let mut msg = String::from("RIFT INVARIANT VIOLATION\n\n");
    for v in violations {
        msg.push_str(&v.to_string());
        msg.push('\n');
    }
    let _ = std::fs::write(std::env::temp_dir().join("rift-violation.log"), &msg);
    let _ = std::fs::write("rift-violation.log", &msg);
}
