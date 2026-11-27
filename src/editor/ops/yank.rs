use super::super::Editor;
use crate::clipboard::Clip;
#[cfg(feature = "plugins")]
use crate::plugin::events::EventPayload;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn yank(&mut self, clip: Clip) {
        self.clipboard_ring.push(clip);
        #[cfg(feature = "plugins")]
        if let Some(buf) = self.document_manager.active_document_id() {
            self.raise(EventPayload::TextYank { buf });
        }
    }
}
