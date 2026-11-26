use super::super::Editor;
#[cfg(feature = "lsp")]
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn rename_lsp_symbol(&mut self, new_name: String) {
        #[cfg(feature = "lsp")]
        match self.cursor_lsp_position() {
            Some((path, line, col)) => {
                if self
                    .lsp_manager
                    .rename(&path, line, col, new_name)
                    .is_none()
                {
                    self.notify_no_lsp_server();
                }
            }
            None => self.notify(
                crate::notification::NotificationType::Warning,
                "LSP rename: no file open".to_string(),
            ),
        }
        #[cfg(not(feature = "lsp"))]
        let _ = new_name;
    }

    #[cfg(feature = "lsp")]
    pub(crate) fn register_lsp_server(
        &mut self,
        language: String,
        config: crate::lsp::config::LspServerConfig,
    ) {
        self.lsp_manager.register_server(language, config);
        self.lsp_notify_open_all();
    }

    pub(crate) fn request_lsp_goto_definition(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        if let Some((path, line, col)) = self.lsp_request_position() {
            if self.lsp_manager.goto_definition(&path, line, col).is_none() {
                self.notify_no_lsp_server();
            }
        }
        true
    }

    pub(crate) fn request_lsp_references(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        if let Some((path, line, col)) = self.lsp_request_position() {
            if self.lsp_manager.references(&path, line, col).is_none() {
                self.notify_no_lsp_server();
            }
        }
        true
    }

    pub(crate) fn request_lsp_hover(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        if let Some((path, line, col)) = self.lsp_request_position() {
            if self.lsp_manager.hover(&path, line, col).is_none() {
                self.notify_no_lsp_server();
            }
        }
        true
    }

    pub(crate) fn request_lsp_rename(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        {
            if let Some(ctx) = self.cursor_lsp_position() {
                self.rename_context = Some(ctx);
                self.state.clear_command_line();
                self.set_mode(Mode::Rename);
            } else {
                self.notify(
                    crate::notification::NotificationType::Warning,
                    "LSP rename: no file open".to_string(),
                );
            }
        }
        true
    }

    pub(crate) fn request_lsp_code_action(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        if let Some((path, line, col)) = self.lsp_request_position() {
            let uri = crate::lsp::protocol::path_to_uri(&path);
            let norm_uri = crate::lsp::protocol::normalize_uri(&uri);
            let diagnostics: Vec<crate::lsp::protocol::LspDiagnostic> =
                self.lsp_diagnostics.get(&norm_uri).map_or_default(|diags| {
                    diags
                        .iter()
                        .filter(|d| d.range.start.line == line)
                        .cloned()
                        .collect()
                });
            if self
                .lsp_manager
                .code_action(&path, line, col, diagnostics)
                .is_none()
            {
                self.notify_no_lsp_server();
            }
        }
        true
    }

    pub(crate) fn request_lsp_format(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        {
            let info = self.document_manager.active_document().and_then(|doc| {
                let path = doc.path()?.to_path_buf();
                let tab_size = u32::try_from(doc.options().tab_width()).unwrap_or(u32::MAX);
                let insert_spaces = doc.options().expand_tabs();
                Some((path, tab_size, insert_spaces))
            });
            if let Some((path, tab_size, insert_spaces)) = info {
                if self.lsp_manager.is_indexing_path(&path) {
                    self.notify(
                        crate::notification::NotificationType::Info,
                        "LSP: still indexing, please wait...".to_string(),
                    );
                } else if self
                    .lsp_manager
                    .format(&path, tab_size, insert_spaces)
                    .is_none()
                {
                    self.notify_no_lsp_server();
                }
            }
        }
        true
    }

    pub(crate) fn request_lsp_diagnostic_next(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        self.lsp_diagnostic_next();
        true
    }

    pub(crate) fn request_lsp_diagnostic_prev(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        self.lsp_diagnostic_prev();
        true
    }

    pub(crate) fn request_lsp_diagnostics_panel(&mut self) -> bool {
        #[cfg(feature = "lsp")]
        self.open_diagnostics_panel();
        true
    }
}
