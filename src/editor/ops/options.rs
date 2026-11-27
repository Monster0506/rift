use super::super::Editor;
use crate::command_line::settings::descriptor::SettingEffect;
use crate::command_line::settings::resolver::{OptionScope, SettingsResolver};
use crate::error::{ErrorType, RiftError};
#[cfg(feature = "plugins")]
use crate::plugin::events::EventPayload;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn set_option(
        &mut self,
        name: &str,
        value: Option<&str>,
        scope: OptionScope,
    ) -> Result<(), RiftError> {
        let local = match scope {
            #[cfg(any(feature = "plugins", test))]
            OptionScope::Global => None,
            OptionScope::Local(id) => Some(id),
            OptionScope::Resolve(id) => {
                SettingsResolver::new(self.document_settings_registry, self.settings_registry)
                    .is_local(name)
                    .then_some(id)
            }
        };
        let applied = if let Some(id) = local {
            let doc = self.document_manager.get_document_mut(id).ok_or_else(|| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    format!("Document {id} not found"),
                )
            })?;
            let descriptor = doc.set_option(name, value)?;
            (descriptor.name, descriptor.effect)
        } else {
            let descriptor = self
                .settings_registry
                .set(name, value, &mut self.state.settings)?;
            (descriptor.name, descriptor.effect)
        };
        match applied.1 {
            SettingEffect::None => {}
            SettingEffect::Redraw => self.request_full_redraw(),
            SettingEffect::ResizeClipboard => {
                let capacity = self.state.settings.clipboard_ring_size();
                if capacity != self.clipboard_ring.capacity() {
                    self.clipboard_ring.set_capacity(capacity);
                    self.refresh_clipboard_buffer_if_open();
                }
            }
        }
        #[cfg(feature = "plugins")]
        self.raise(EventPayload::OptionSet {
            buf: local,
            name: applied.0,
        });
        Ok(())
    }
}
