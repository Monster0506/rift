//! Global constants for the Rift editor

pub mod paths {
    /// Directory name for grammar files
    pub const GRAMMARS_DIR: &str = "grammars";
}

pub mod ui {
    /// Display text for documents with no file path
    pub const NO_NAME: &str = "[No Name]";

    /// Marker for undo tree nodes with snapshots
    pub const SNAPSHOT_MARKER: &str = "*";

    /// Border character sets for UI components
    pub mod borders {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct BorderSet {
            pub top_left: char,
            pub top_right: char,
            pub bottom_left: char,
            pub bottom_right: char,
            pub horizontal: char,
            pub vertical: char,
        }

        pub const UNICODE: BorderSet = BorderSet {
            top_left: '╭',
            top_right: '╮',
            bottom_left: '╰',
            bottom_right: '╯',
            horizontal: '─',
            vertical: '│',
        };

        pub const ASCII: BorderSet = BorderSet {
            top_left: '+',
            top_right: '+',
            bottom_left: '+',
            bottom_right: '+',
            horizontal: '-',
            vertical: '|',
        };
    }
}

pub mod errors {
    // Error Codes
    pub const LOAD_FAILED: &str = "LOAD_FAILED";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    pub const UNSAVED_CHANGES: &str = "UNSAVED_CHANGES";
    pub const NO_PATH: &str = "NO_PATH";
    pub const READ_ONLY: &str = "READ_ONLY";
    pub const SEARCH_ERROR: &str = "SEARCH_ERROR";
    pub const UNDO_ERROR: &str = "UNDO_ERROR";
    pub const REDRAW_FAILED: &str = "REDRAW_FAILED";
    pub const POLL_FAILED: &str = "POLL_FAILED";
    pub const RENDER_FAILED: &str = "RENDER_FAILED";
    pub const REGEX_PARSE_ERROR: &str = "REGEX_PARSE_ERROR";
    pub const REGEX_COMPILE_ERROR: &str = "REGEX_COMPILE_ERROR";
    pub const UTF8_ERROR: &str = "UTF8_ERROR";

    // Error Messages
    pub const MSG_UNSAVED_CHANGES: &str = "No write since last change (add ! to override)";
    pub const MSG_NO_FILE_NAME: &str = "No file name";
    pub const MSG_FILE_NOT_FOUND_WIN: &str = "The system cannot find the file specified";
}

pub mod history {
    pub const INSERT_LABEL: &str = "Insert";
    pub const ALREADY_OLDEST: &str = "Already at oldest change";
    pub const ALREADY_NEWEST: &str = "Already at newest change";
}

pub mod modes {
    pub const NORMAL: &str = "NORMAL";
    pub const INSERT: &str = "INSERT";
    pub const COMMAND: &str = "COMMAND";
    pub const SEARCH: &str = "SEARCH";
    pub const OVERLAY: &str = "OVERLAY";
}

pub mod logging {
    pub const INFO: &str = "INFO";
    pub const WARN: &str = "WARN";
    pub const ERROR: &str = "ERROR";
    pub const CRITICAL: &str = "CRITICAL";
}

pub mod error_types {
    pub const IO: &str = "IO";
    pub const INVALID_CURSOR: &str = "INVALID_CURSOR";
    pub const PARSE: &str = "PARSE";
    pub const SETTINGS: &str = "SETTINGS";
    pub const EXECUTION: &str = "EXECUTION";
    pub const RENDERER: &str = "RENDERER";
    pub const INTERNAL: &str = "INTERNAL";
    pub const OTHER: &str = "OTHER";
}

pub mod themes {
    pub const LIGHT: &str = "light";
    pub const DARK: &str = "dark";
    pub const GRUVBOX: &str = "gruvbox";
    pub const NORDIC: &str = "nordic";
}

pub mod captures {
    pub const ATTRIBUTE: &str = "attribute";
    pub const COMMENT: &str = "comment";
    pub const CONSTANT: &str = "constant";
    pub const CONSTRUCTOR: &str = "constructor";
    pub const EMBEDDED: &str = "embedded";
    pub const FUNCTION: &str = "function";
    pub const KEYWORD: &str = "keyword";
    pub const LABEL: &str = "label";
    pub const OPERATOR: &str = "operator";
    pub const PROPERTY: &str = "property";
    pub const PUNCTUATION: &str = "punctuation";
    pub const STRING: &str = "string";
    pub const TYPE: &str = "type";
    pub const VARIABLE: &str = "variable";
    pub const TAG: &str = "tag";

    // Additional captures used in render
    pub const BOOLEAN: &str = "boolean";
    pub const BUILTIN: &str = "builtin";
    pub const ESCAPE: &str = "escape";
    pub const FIELD: &str = "field";
    pub const MODULE: &str = "module";
    pub const NAMESPACE: &str = "namespace";
    pub const NUMBER: &str = "number";
}
