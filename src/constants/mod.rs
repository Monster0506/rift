pub(crate) mod paths {
    pub(crate) const GRAMMARS_DIR: &str = "grammars";
}

pub(crate) mod ui {
    pub(crate) mod borders {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(crate) struct BorderSet {
            pub(crate) top_left: char,
            pub(crate) top_right: char,
            pub(crate) bottom_left: char,
            pub(crate) bottom_right: char,
            pub(crate) horizontal: char,
            pub(crate) vertical: char,
        }

        pub(crate) const UNICODE: BorderSet = BorderSet {
            top_left: '╭',
            top_right: '╮',
            bottom_left: '╰',
            bottom_right: '╯',
            horizontal: '─',
            vertical: '│',
        };
    }
}

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

pub(crate) mod history {
    pub(crate) const INSERT_LABEL: &str = "Insert";
}

pub(crate) mod modes {
    pub(crate) const NORMAL: &str = "NORMAL";
    pub(crate) const INSERT: &str = "INSERT";
    pub(crate) const COMMAND: &str = "COMMAND";
    pub(crate) const SEARCH: &str = "SEARCH";
    pub(crate) const RENAME: &str = "RENAME";
    pub(crate) const OPERATOR_PENDING: &str = "OPERATOR_PENDING";
    pub(crate) const REPLACE: &str = "REPLACE";
    pub(crate) const VISUAL: &str = "VISUAL";
    pub(crate) const VISUAL_LINE: &str = "VISUAL_LINE";
    pub(crate) const VISUAL_BLOCK: &str = "VISUAL_BLOCK";
}

pub(crate) mod logging {
    #[cfg(test)]
    pub(crate) const INFO: &str = "INFO";
    pub(crate) const WARN: &str = "WARN";
    pub(crate) const ERROR: &str = "ERROR";
    #[cfg(test)]
    pub(crate) const CRITICAL: &str = "CRITICAL";
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

pub(crate) mod captures {
    pub(crate) const ATTRIBUTE: &str = "attribute";
    pub(crate) const BOOLEAN: &str = "boolean";
    pub(crate) const BUILTIN: &str = "builtin";
    pub(crate) const COMMENT: &str = "comment";
    pub(crate) const CONSTANT: &str = "constant";
    pub(crate) const CONSTRUCTOR: &str = "constructor";
    pub(crate) const ESCAPE: &str = "escape";
    pub(crate) const FUNCTION: &str = "function";
    pub(crate) const KEYWORD: &str = "keyword";
    pub(crate) const LABEL: &str = "label";
    pub(crate) const MODULE: &str = "module";
    pub(crate) const NAMESPACE: &str = "namespace";
    pub(crate) const NUMBER: &str = "number";
    pub(crate) const OPERATOR: &str = "operator";
    pub(crate) const PARAMETER: &str = "parameter";
    pub(crate) const PROPERTY: &str = "property";
    pub(crate) const PUNCTUATION: &str = "punctuation";
    pub(crate) const STRING: &str = "string";
    pub(crate) const TAG: &str = "tag";
    pub(crate) const TEXT_LITERAL: &str = "text.literal";
    pub(crate) const TEXT_REFERENCE: &str = "text.reference";
    pub(crate) const TEXT_TITLE: &str = "text.title";
    pub(crate) const TEXT_URI: &str = "text.uri";
    pub(crate) const TYPE: &str = "type";
    pub(crate) const VARIABLE: &str = "variable";
    pub(crate) const C_IMPORT: &str = "cImport";
    pub(crate) const CHARACTER: &str = "character";
    pub(crate) const MODULE_BUILTIN: &str = "module.builtin";
    pub(crate) const SPELL: &str = "spell";
}
