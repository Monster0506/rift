use super::Document;
use crate::syntax::{ParseOutcome, SYNC_PARSE_BUDGET, SYNC_PARSE_MAX_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxSync {
    NoSyntax,
    Completed,
    Deferred,
}

impl Document {
    pub fn resync_syntax(&mut self) -> SyntaxSync {
        let Some(syntax) = self.syntax.as_mut() else {
            return SyntaxSync::NoSyntax;
        };
        if self.buffer.byte_len() > SYNC_PARSE_MAX_BYTES {
            return SyntaxSync::Deferred;
        }
        let source = self.buffer.to_logical_bytes();
        match syntax.try_incremental_parse(&source, SYNC_PARSE_BUDGET) {
            ParseOutcome::Completed => SyntaxSync::Completed,
            ParseOutcome::Aborted => SyntaxSync::Deferred,
            ParseOutcome::NoLanguage => SyntaxSync::NoSyntax,
        }
    }
}
