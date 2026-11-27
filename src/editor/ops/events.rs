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
        } else {
            self.dispatch_events(&[event]);
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
