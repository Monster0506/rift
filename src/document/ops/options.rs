use super::super::definitions::{create_document_settings_registry, DocumentOptions};
use super::super::Document;
use crate::command_line::settings::descriptor::SettingDescriptor;
use crate::error::RiftError;
use crate::tracked::Tracked;

impl Document {
    #[must_use]
    pub(crate) const fn options(&self) -> &Tracked<DocumentOptions> {
        &self.options
    }

    pub(crate) fn set_option(
        &mut self,
        name: &str,
        value: Option<&str>,
    ) -> Result<&'static SettingDescriptor<DocumentOptions>, RiftError> {
        create_document_settings_registry().set(name, value, &mut self.options)
    }
}
