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
    pub const INVALID_CURSOR: &str = "InvalidCursor";
    pub const PARSE: &str = "Parse";
    pub const SETTINGS: &str = "Settings";
    pub const EXECUTION: &str = "Execution";
    pub const RENDERER: &str = "Renderer";
    pub const INTERNAL: &str = "Internal";
    pub const OTHER: &str = "Other";
}

pub mod themes {
    pub const LIGHT: &str = "light";
    pub const DARK: &str = "dark";
    pub const GRUVBOX: &str = "gruvbox";
    pub const NORDIC: &str = "nordic";
}

pub mod captures {
    pub const ATTRIBUTE: &str = "attribute";
    pub const ATTRIBUTE_ITEM: &str = "attribute_item";
    pub const ANCHOR_NAME: &str = "anchor_name";
    pub const ALIAS_NAME: &str = "alias_name";
    pub const BOOLEAN: &str = "boolean";
    pub const BOOLEAN_LITERAL: &str = "boolean_literal";
    pub const BOOLEAN_SCALAR: &str = "boolean_scalar";
    pub const BLOCK_COMMENT: &str = "block_comment";
    pub const BLOCK_SCALAR: &str = "block_scalar";
    pub const BUILTIN: &str = "builtin";
    pub const CHAR_LITERAL: &str = "char_literal";
    pub const COMMENT: &str = "comment";
    pub const COMMENT_DOCUMENTATION: &str = "comment.documentation";
    pub const CONSTANT: &str = "constant";
    pub const CONSTANT_BUILTIN: &str = "constant.builtin";
    pub const CONSTRUCTOR: &str = "constructor";
    pub const CRATE: &str = "crate";
    pub const DECORATOR: &str = "decorator";
    pub const DELIMITER: &str = "delimiter";
    pub const DOUBLE_QUOTE_SCALAR: &str = "double_quote_scalar";
    pub const EMBEDDED: &str = "embedded";
    pub const ERROR: &str = "error";
    pub const ESCAPE: &str = "escape";
    pub const ESCAPE_SEQUENCE: &str = "escape_sequence";
    pub const FIELD: &str = "field";
    pub const FIELD_IDENTIFIER: &str = "field_identifier";
    pub const FLOAT: &str = "float";
    pub const FLOAT_LITERAL: &str = "float_literal";
    pub const FLOAT_SCALAR: &str = "float_scalar";
    pub const FLOW_MAPPING: &str = "flow_mapping";
    pub const FLOW_NODE: &str = "flow_node";
    pub const FUNCTION: &str = "function";
    pub const FUNCTION_BRACKET: &str = "function.bracket";
    pub const FUNCTION_BUILTIN: &str = "function.builtin";
    pub const FUNCTION_CALL: &str = "function.call";
    pub const FUNCTION_CALL_LUA: &str = "function.call.lua";
    pub const FUNCTION_MACRO: &str = "function.macro";
    pub const FUNCTION_METHOD: &str = "function.method";
    pub const FUNCTION_ITEM: &str = "function_item";
    pub const FUNCTION_SIGNATURE_ITEM: &str = "function_signature_item";
    pub const GENERIC_FUNCTION: &str = "generic_function";
    pub const INNER_ATTRIBUTE_ITEM: &str = "inner_attribute_item";
    pub const INTEGER: &str = "integer";
    pub const INTEGER_LITERAL: &str = "integer_literal";
    pub const INTEGER_SCALAR: &str = "integer_scalar";
    pub const INTERPOLATION: &str = "interpolation";
    pub const KEYWORD: &str = "keyword";
    pub const KEYWORD_CONDITIONAL: &str = "keyword.conditional";
    pub const KEYWORD_FUNCTION: &str = "keyword.function";
    pub const KEYWORD_OPERATOR: &str = "keyword.operator";
    pub const KEYWORD_REPEAT: &str = "keyword.repeat";
    pub const KEYWORD_RETURN: &str = "keyword.return";
    pub const LABEL: &str = "label";
    pub const LIFETIME: &str = "lifetime";
    pub const LINE_COMMENT: &str = "line_comment";
    pub const MACRO_INVOCATION: &str = "macro_invocation";
    pub const MODULE: &str = "module";
    pub const MUTABLE_SPECIFIER: &str = "mutable_specifier";
    pub const NAMESPACE: &str = "namespace";
    pub const NESTING_SELECTOR: &str = "nesting_selector";
    pub const NONE: &str = "none";
    pub const NULL_SCALAR: &str = "null_scalar";
    pub const NUMBER: &str = "number";
    pub const OPERATOR: &str = "operator";
    pub const PARAMETER: &str = "parameter";
    pub const PLAIN_SCALAR: &str = "plain_scalar";
    pub const PROPERTY: &str = "property";
    pub const PROPERTY_IDENTIFIER: &str = "property_identifier";
    pub const PUNCTUATION: &str = "punctuation";
    pub const PUNCTUATION_BRACKET: &str = "punctuation.bracket";
    pub const PUNCTUATION_DELIMITER: &str = "punctuation.delimiter";
    pub const PUNCTUATION_SPECIAL: &str = "punctuation.special";
    pub const RAW_STRING_LITERAL: &str = "raw_string_literal";
    pub const RESERVED_DIRECTIVE: &str = "reserved_directive";
    pub const SCOPED_IDENTIFIER: &str = "scoped_identifier";
    pub const SCOPED_TYPE_IDENTIFIER: &str = "scoped_type_identifier";
    pub const SELF: &str = "self";
    pub const SINGLE_QUOTE_SCALAR: &str = "single_quote_scalar";
    pub const STRING: &str = "string";
    pub const STRING_ESCAPE: &str = "string.escape";
    pub const STRING_SPECIAL: &str = "string.special";
    pub const STRING_SPECIAL_KEY: &str = "string.special.key";
    pub const STRING_LITERAL: &str = "string_literal";
    pub const STRING_SCALAR: &str = "string_scalar";
    pub const STRUCT_PATTERN: &str = "struct_pattern";
    pub const TAG: &str = "tag";
    pub const TAG_ERROR: &str = "tag.error";
    pub const TAG_DIRECTIVE: &str = "tag_directive";
    pub const TEXT_LITERAL: &str = "text.literal";
    pub const TEXT_REFERENCE: &str = "text.reference";
    pub const TEXT_TITLE: &str = "text.title";
    pub const TEXT_URI: &str = "text.uri";
    pub const TYPE: &str = "type";
    pub const TYPE_BUILTIN: &str = "type.builtin";
    pub const TYPE_ARGUMENTS: &str = "type_arguments";
    pub const TYPE_IDENTIFIER: &str = "type_identifier";
    pub const TYPE_PARAMETERS: &str = "type_parameters";
    pub const UNIVERSAL_SELECTOR: &str = "universal_selector";
    pub const VARIABLE: &str = "variable";
    pub const VARIABLE_BUILTIN: &str = "variable.builtin";
    pub const VARIABLE_PARAMETER: &str = "variable.parameter";
    // Explicit keywords found in coverage test
    pub const KEYWORD_IMPORT: &str = "import";
    pub const KEYWORD_DIRECTIVE: &str = "keyword.directive";
    pub const C_IMPORT: &str = "cImport";
    pub const CHARACTER: &str = "character";
    pub const MODULE_BUILTIN: &str = "module.builtin";
    pub const SPELL: &str = "spell";
}
