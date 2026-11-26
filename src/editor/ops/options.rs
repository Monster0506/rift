use super::super::Editor;
use crate::command_line::settings::resolver::{OptionScope, SettingsResolver};
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn set_option(
        &mut self,
        name: &str,
        value: Option<&str>,
        scope: OptionScope,
    ) -> Result<bool, RiftError> {
        let local = match scope {
            OptionScope::Global => None,
            OptionScope::Local(id) => Some(id),
            OptionScope::Resolve(id) => {
                SettingsResolver::new(self.document_settings_registry, self.settings_registry)
                    .is_local(name)
                    .then_some(id)
            }
        };
        let descriptor_redraws = match local {
            Some(id) => {
                let doc = self.document_manager.get_document_mut(id).ok_or_else(|| {
                    RiftError::new(
                        ErrorType::Internal,
                        crate::constants::errors::INTERNAL_ERROR,
                        format!("Document {id} not found"),
                    )
                })?;
                doc.set_option(name, value)?.needs_full_redraw
            }
            None => {
                self.settings_registry
                    .set(name, value, &mut self.state.settings)?
                    .needs_full_redraw
            }
        };
        Ok(descriptor_redraws)
    }
}
