use super::super::Editor;
use crate::plugin::events::{EventPayload, SaveDecision};
use crate::term::TerminalBackend;

#[repr(transparent)]
pub(crate) struct EditorEvent(EventPayload);

impl EditorEvent {
    pub(crate) const fn payload(&self) -> &EventPayload {
        &self.0
    }

    #[cfg(test)]
    pub(crate) const fn for_test(payload: EventPayload) -> Self {
        Self(payload)
    }
}

fn same_subject(a: &EventPayload, b: &EventPayload) -> bool {
    use EventPayload as E;
    match (a, b) {
        (E::BufOpen { buf: x, .. }, E::BufOpen { buf: y, .. })
        | (E::TextChangedCoarse { buf: x }, E::TextChangedCoarse { buf: y })
        | (E::SearchChanged { buf: x }, E::SearchChanged { buf: y })
        | (E::TextYank { buf: x }, E::TextYank { buf: y })
        | (E::CursorMoved { buf: x, .. }, E::CursorMoved { buf: y, .. })
        | (E::SelectionChanged { buf: x }, E::SelectionChanged { buf: y }) => x == y,
        #[cfg(feature = "treesitter")]
        (E::SyntaxUpdated { buf: x, .. }, E::SyntaxUpdated { buf: y, .. }) => x == y,
        #[cfg(feature = "lsp")]
        (E::LspDiagnosticsChanged { uri: x, .. }, E::LspDiagnosticsChanged { uri: y, .. }) => {
            x == y
        }
        #[cfg(feature = "lsp")]
        (
            E::LspProgress {
                language: lx,
                server_name: sx,
            },
            E::LspProgress {
                language: ly,
                server_name: sy,
            },
        ) => lx == ly && sx == sy,
        (E::WindowResized { .. }, E::WindowResized { .. }) => true,
        _ => false,
    }
}

fn delivered_at_frame(payload: &EventPayload) -> bool {
    match payload {
        EventPayload::BufOpen { .. }
        | EventPayload::TextChangedCoarse { .. }
        | EventPayload::SearchChanged { .. }
        | EventPayload::TextYank { .. }
        | EventPayload::CursorMoved { .. }
        | EventPayload::SelectionChanged { .. }
        | EventPayload::WindowResized { .. } => true,
        #[cfg(feature = "treesitter")]
        EventPayload::SyntaxUpdated { .. } => true,
        _ => is_lsp_frame_event(payload),
    }
}

#[cfg(feature = "lsp")]
const fn is_lsp_frame_event(payload: &EventPayload) -> bool {
    matches!(
        payload,
        EventPayload::LspDiagnosticsChanged { .. } | EventPayload::LspProgress { .. }
    )
}

#[cfg(not(feature = "lsp"))]
const fn is_lsp_frame_event(_payload: &EventPayload) -> bool {
    false
}

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn raise(&mut self, payload: EventPayload) -> SaveDecision {
        let event = EditorEvent(payload);
        if matches!(event.payload(), EventPayload::BufSavePre { .. }) {
            return self.call_plugins(|editor| editor.plugin_host.dispatch(&event));
        }
        if let EventPayload::CursorHold { now, .. } = event.payload() {
            self.call_plugins(|editor| {
                editor.plugin_host.dispatch(&event);
                editor.plugin_host.fire_due_lua_timers(*now);
            });
        } else if delivered_at_frame(event.payload()) {
            if let Some(queued) = self
                .frame_events
                .iter_mut()
                .find(|queued| same_subject(queued.payload(), event.payload()))
            {
                *queued = event;
            } else {
                self.frame_events.push(event);
            }
        } else {
            self.dispatch_events(&[event]);
        }
        SaveDecision::Proceed
    }

    pub(super) fn raise_save_pre(
        &mut self,
        buf: crate::document::DocumentId,
        path: Option<std::path::PathBuf>,
    ) -> SaveDecision {
        self.raise(EventPayload::BufSavePre { buf, path })
    }

    pub(super) fn deliver_open_before_close(&mut self, id: crate::document::DocumentId) {
        if let Some(index) = self.frame_events.iter().position(
            |event| matches!(event.payload(), EventPayload::BufOpen { buf, .. } if *buf == id),
        ) {
            let event = self.frame_events.remove(index);
            self.dispatch_events(&[event]);
        }
    }

    pub(in crate::editor) fn raise_buf_open(&mut self, id: crate::document::DocumentId) {
        let Some(doc) = self.document_manager.get_document(id) else {
            return;
        };
        let path = doc.path().map(std::path::Path::to_path_buf);
        let filetype = doc
            .syntax
            .as_ref()
            .map(|s| s.language_name.clone())
            .or_else(|| {
                path.as_deref()
                    .and_then(|p| self.language_loader.language_name_for_file(p))
            });
        self.raise(EventPayload::BufOpen {
            buf: id,
            path,
            filetype,
        });
    }

    #[cfg(feature = "treesitter")]
    pub(crate) fn raise_syntax_updated(&mut self, buf: crate::document::DocumentId) {
        let lang = self
            .document_manager
            .get_document(buf)
            .and_then(|doc| doc.syntax.as_ref())
            .map(|syntax| syntax.language_name.clone());
        self.raise(EventPayload::SyntaxUpdated { buf, lang });
    }

    pub(in crate::editor) fn dispatch_frame_events(&mut self) {
        let mut events = std::mem::take(&mut self.frame_events);
        crate::perf_span!(
            "plugin_dispatch_buffer_changes",
            crate::perf::PerfFields::default()
        );
        self.dispatch_events(&events);
        if self.frame_events.is_empty() && events.capacity() > self.frame_events.capacity() {
            events.clear();
            self.frame_events = events;
        }
    }

    fn dispatch_events(&mut self, events: &[EditorEvent]) {
        if events.is_empty() {
            return;
        }
        self.call_plugins(|editor| {
            for event in events {
                editor.plugin_host.dispatch(event);
            }
        });
    }
}
