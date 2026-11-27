use std::collections::HashMap;

use super::super::definitions::{create_document_settings_registry, DocumentOptions};
use super::super::Document;
use crate::annotations::value::Value;
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

    pub(crate) const fn vars(&self) -> &Tracked<HashMap<String, Value>> {
        &self.vars
    }

    pub(crate) fn set_var(&mut self, name: String, value: Value) {
        self.vars.insert(name, value);
    }

    pub(crate) fn initialize_vars(&mut self, vars: Vec<(String, Value)>) {
        *self.vars = vars.into_iter().collect();
    }
}
