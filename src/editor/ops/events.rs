#![allow(clippy::disallowed_methods)]

use super::super::Editor;
use crate::plugin::events::EventPayload;
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

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn raise(&mut self, payload: EventPayload) {
        let event = EditorEvent(payload);
        if let EventPayload::CursorHold { now, .. } = event.payload() {
            self.call_plugins(|editor| {
                editor.plugin_host.dispatch(&event);
                editor.plugin_host.fire_due_lua_timers(*now);
            });
        } else if matches!(
            event.payload(),
            EventPayload::BufOpen { .. }
                | EventPayload::TextChangedCoarse { .. }
                | EventPayload::CursorMoved { .. }
                | EventPayload::SelectionChanged { .. }
                | EventPayload::SyntaxUpdated { .. }
                | EventPayload::LspDiagnosticsChanged { .. }
                | EventPayload::LspProgress { .. }
                | EventPayload::WindowResized { .. }
        ) {
            self.frame_events.push(event);
        } else {
            self.dispatch_events(&[event]);
        }
    }

    pub(super) fn deliver_open_before_close(&mut self, id: crate::document::DocumentId) {
        if let Some(index) = self.frame_events.iter().position(
            |event| matches!(event.payload(), EventPayload::BufOpen { buf, .. } if *buf == id),
        ) {
            let event = self.frame_events.remove(index);
            self.dispatch_events(&[event]);
        }
    }

    pub(crate) fn deliver_frame_events(&mut self) {
        self.deliver_document_changes();
        self.drain_syntax_sync();
        self.flush_pending_syntax_updated();
        let events = std::mem::take(&mut self.frame_events);
        crate::perf_span!(
            "plugin_dispatch_buffer_changes",
            crate::perf::PerfFields::default()
        );
        self.dispatch_events(&events);
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
