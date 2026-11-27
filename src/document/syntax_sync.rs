use super::Document;
use crate::syntax::{ParseOutcome, SYNC_PARSE_BUDGET, SYNC_PARSE_MAX_BYTES};

#[derive(Debug, Default)]
pub(crate) struct SyntaxSettle {
    stale: bool,
    notice: Option<SyntaxNotice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxNotice {
    #[cfg(feature = "treesitter")]
    Completed,
    Deferred,
}

impl Document {
    pub(in crate::document) const fn mark_syntax_stale(&mut self) {
        self.syntax_settle.stale = true;
    }

    pub(in crate::document) fn settle_syntax(&mut self) {
        if self.transaction_depth > 0 || !self.syntax_settle.stale {
            return;
        }
        self.syntax_settle.stale = false;
        self.syntax_settle.notice = self.syntax.as_mut().and_then(|syntax| {
            if self.buffer.byte_len() > SYNC_PARSE_MAX_BYTES {
                return Some(SyntaxNotice::Deferred);
            }
            match syntax.try_incremental_parse(&self.buffer, SYNC_PARSE_BUDGET) {
                #[cfg(feature = "treesitter")]
                ParseOutcome::Completed => Some(SyntaxNotice::Completed),
                #[cfg(feature = "treesitter")]
                ParseOutcome::Aborted => Some(SyntaxNotice::Deferred),
                ParseOutcome::NoLanguage => None,
            }
        });
    }

    #[cfg(feature = "lsp")]
    pub(crate) fn invalidate_syntax(&mut self) {
        if let Some(syntax) = &mut self.syntax {
            syntax.invalidate_trees();
            self.reparse_syntax();
        }
    }

    pub(crate) fn reparse_syntax(&mut self) {
        self.mark_syntax_stale();
        self.settle_syntax();
    }

    pub(crate) fn take_syntax_notice(&mut self) -> Option<SyntaxNotice> {
        self.syntax_settle.notice.take()
    }
}
