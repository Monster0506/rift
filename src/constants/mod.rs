pub(crate) mod errors {
    pub(crate) const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    pub(crate) const UNSAVED_CHANGES: &str = "UNSAVED_CHANGES";
    pub(crate) const NO_PATH: &str = "NO_PATH";
    pub(crate) const NOT_A_FILE: &str = "NOT_A_FILE";
    pub(crate) const NOT_A_DIRECTORY: &str = "NOT_A_DIRECTORY";
    pub(crate) const SEARCH_ERROR: &str = "SEARCH_ERROR";
    pub(crate) const PLUGIN_LOAD_FAILED: &str = "PLUGIN_LOAD_FAILED";
    pub(crate) const POLL_FAILED: &str = "POLL_FAILED";
    pub(crate) const RENDER_FAILED: &str = "RENDER_FAILED";
    pub(crate) const PARENT_DIR_MISSING: &str = "PARENT_DIR_MISSING";
    pub(crate) const UNDOFILE_CORRUPT: &str = "UNDOFILE_CORRUPT";
    pub(crate) const RELOAD_UNSUPPORTED: &str = "RELOAD_UNSUPPORTED";

    pub(crate) const MSG_UNSAVED_CHANGES: &str = "No write since last change (add ! to override)";
    pub(crate) const MSG_NO_FILE_NAME: &str = "No file name";
    pub(crate) const MSG_NOT_A_FILE: &str = "Cannot edit a directory";
    pub(crate) const MSG_NOT_A_DIRECTORY: &str = "Not a directory; :file expects a directory path";
    pub(crate) const MSG_PARENT_DIR_MISSING: &str =
        "No such directory; create the parent directory first";
    pub(crate) const MSG_RELOAD_UNSUPPORTED: &str = "This buffer cannot be reloaded";
}

pub(crate) mod error_types {
    pub(crate) const IO: &str = "IO";
    pub(crate) const INVALID_CURSOR: &str = "InvalidCursor";
    pub(crate) const PARSE: &str = "Parse";
    pub(crate) const SETTINGS: &str = "Settings";
    pub(crate) const EXECUTION: &str = "Execution";
    pub(crate) const RENDERER: &str = "Renderer";
    pub(crate) const INTERNAL: &str = "Internal";
    pub(crate) const OTHER: &str = "Other";
}

pub(crate) mod themes {
    pub(crate) const LIGHT: &str = "light";
    pub(crate) const DARK: &str = "dark";
    pub(crate) const GRUVBOX: &str = "gruvbox";
    pub(crate) const NORDIC: &str = "nordic";
}
