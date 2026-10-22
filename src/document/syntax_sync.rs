use super::Document;
use crate::syntax::{ParseOutcome, SYNC_PARSE_BUDGET, SYNC_PARSE_MAX_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyntaxSync {
    NoSyntax,
    #[cfg(feature = "treesitter")]
    Completed,
    Deferred,
}

impl Document {
    pub(crate) fn resync_syntax(&mut self) -> SyntaxSync {
        let Some(syntax) = self.syntax.as_mut() else {
            return SyntaxSync::NoSyntax;
        };
        if self.buffer.byte_len() > SYNC_PARSE_MAX_BYTES {
            return SyntaxSync::Deferred;
        }
        let source = self.buffer.to_logical_bytes();
        match syntax.try_incremental_parse(&source, SYNC_PARSE_BUDGET) {
            #[cfg(feature = "treesitter")]
            ParseOutcome::Completed => SyntaxSync::Completed,
            #[cfg(feature = "treesitter")]
            ParseOutcome::Aborted => SyntaxSync::Deferred,
            ParseOutcome::NoLanguage => SyntaxSync::NoSyntax,
        }
    }
}
