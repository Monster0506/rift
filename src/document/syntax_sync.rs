use super::Document;
use crate::syntax::{ParseOutcome, SYNC_PARSE_BUDGET, SYNC_PARSE_MAX_BYTES};

#[derive(Debug, Default)]
pub(crate) struct SyntaxSettle {
    stale: bool,
    notice: SyntaxNotice,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxNotice {
    pub(crate) completed: bool,
    pub(crate) deferred: bool,
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
        let Some(syntax) = self.syntax.as_mut() else {
            return;
        };
        if self.buffer.byte_len() > SYNC_PARSE_MAX_BYTES {
            self.syntax_settle.notice.deferred = true;
            return;
        }
        match syntax.try_incremental_parse(&self.buffer, SYNC_PARSE_BUDGET) {
            #[cfg(feature = "treesitter")]
            ParseOutcome::Completed => {
                self.syntax_settle.notice = SyntaxNotice {
                    completed: true,
                    deferred: false,
                };
            }
            #[cfg(feature = "treesitter")]
            ParseOutcome::Aborted => self.syntax_settle.notice.deferred = true,
            ParseOutcome::NoLanguage => {}
        }
    }

    pub(crate) fn reparse_syntax(&mut self) {
        self.mark_syntax_stale();
        self.settle_syntax();
    }

    pub(crate) fn take_syntax_notice(&mut self) -> Option<SyntaxNotice> {
        let notice = std::mem::take(&mut self.syntax_settle.notice);
        (notice != SyntaxNotice::default()).then_some(notice)
    }
}
