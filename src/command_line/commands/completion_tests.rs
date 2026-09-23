use super::*;
use crate::command_line::settings::create_settings_registry;
use crate::document::definitions::{create_document_settings_registry, DocumentOptions};
use crate::state::UserSettings;

#[test]
fn test_complete_command_name_prefix() {
    let result = complete_command_name("q");
    assert!(result.candidates.iter().any(|c| c.text == "quit"));
}

#[test]
fn test_complete_command_name_empty_returns_all() {
    let result = complete_command_name("");
    assert!(!result.candidates.is_empty());
}

#[test]
fn test_complete_command_name_alias() {
    let result = complete_command_name("w");
    assert!(result
        .candidates
        .iter()
        .any(|c| c.text == "write" || c.text == "w"));
}

#[test]
fn test_complete_command_name_f_prefix() {
    let result = complete_command_name("f");
    assert!(result.candidates.iter().any(|c| c.text == "file"));
}

#[test]
fn test_parse_context_colon_prefix_stripped() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context(":f", &reg, &doc_reg);
    assert_eq!(pc.context, CompletionContext::CommandName);
    assert_eq!(pc.prefix, "f");
}

#[test]
fn test_parse_context_f_space_directories_only() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context(":f ", &reg, &doc_reg);
    assert!(matches!(
        pc.context,
        CompletionContext::FilePath {
            filter: PathFilter::DirectoriesOnly,
            ..
        }
    ));
}

#[test]
fn test_parse_context_e_space_shows_both() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context(":e ", &reg, &doc_reg);
    assert!(matches!(
        pc.context,
        CompletionContext::FilePath {
            filter: PathFilter::Both,
            ..
        }
    ));
}

#[test]
fn test_complete_subcommand() {
    let result = complete_subcommand("buffer", "n");
    assert!(result.candidates.iter().any(|c| c.text == "next"));
}

#[test]
fn test_complete_subcommand_empty() {
    let result = complete_subcommand("buffer", "");
    assert!(!result.candidates.is_empty());
}

#[test]
fn test_complete_subcommand_unknown_parent() {
    let result = complete_subcommand("nonexistent", "");
    assert!(result.candidates.is_empty());
}

#[test]
fn test_longest_common_prefix_multiple() {
    assert_eq!(longest_common_prefix_of(&["write", "wq"]), "w");
}

#[test]
fn test_longest_common_prefix_single() {
    assert_eq!(longest_common_prefix_of(&["quit"]), "quit");
}

#[test]
fn test_longest_common_prefix_empty() {
    assert_eq!(longest_common_prefix_of(&[]), "");
}

#[test]
fn test_split_path_prefix_with_dir() {
    let (dir, prefix) = split_path_prefix("src/f");
    assert_eq!(dir, "src");
    assert_eq!(prefix, "f");
}

#[test]
fn test_split_path_prefix_root_only() {
    let (dir, prefix) = split_path_prefix("foo");
    assert_eq!(dir, ".");
    assert_eq!(prefix, "foo");
}

#[test]
fn test_split_path_prefix_trailing_slash() {
    let (dir, prefix) = split_path_prefix("src/");
    assert_eq!(dir, "src");
    assert_eq!(prefix, "");
}

#[test]
fn test_parse_context_command_name() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("q", &reg, &doc_reg);
    assert_eq!(pc.context, CompletionContext::CommandName);
    assert_eq!(pc.prefix, "q");
    assert_eq!(pc.token_start, 0);
}

#[test]
fn test_parse_context_setting_name() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("set nu", &reg, &doc_reg);
    assert_eq!(pc.context, CompletionContext::SettingName);
    assert_eq!(pc.prefix, "nu");
}

#[test]
fn test_parse_context_subcommand() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("buffer n", &reg, &doc_reg);
    assert_eq!(
        pc.context,
        CompletionContext::Subcommand {
            parent: "buffer".into(),
            subcommand_prefix: String::new(),
        }
    );
    assert_eq!(pc.prefix, "n");
}

#[test]
fn test_parse_context_filepath() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("edit src/f", &reg, &doc_reg);
    assert!(matches!(pc.context, CompletionContext::FilePath { .. }));
}

#[test]
fn test_setting_value_boolean() {
    let reg = create_settings_registry();
    let bool_setting = reg
        .descriptors()
        .iter()
        .find(|d| matches!(d.ty, SettingType::Boolean))
        .expect("at least one boolean setting");
    let result = complete_setting_value::<UserSettings>(bool_setting.name, &reg, None);
    assert_eq!(result.candidates.len(), 2);
    assert!(result.candidates.iter().any(|c| c.text == "true"));
    assert!(result.candidates.iter().any(|c| c.text == "false"));
}

#[test]
fn test_setting_name_no_prefix() {
    let reg = create_settings_registry();
    let bool_setting = reg
        .descriptors()
        .iter()
        .find(|d| matches!(d.ty, SettingType::Boolean))
        .expect("at least one boolean setting");
    let result = complete_setting_name(&format!("no{}", bool_setting.name), &reg);
    let no_name = format!("no{}", bool_setting.name);
    assert!(
        result.candidates.iter().any(|c| c.text == no_name),
        "expected candidate {no_name}"
    );
}

#[test]
fn test_parse_context_split_colon_subcommand() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("split :l", &reg, &doc_reg);
    assert_eq!(
        pc.context,
        CompletionContext::Subcommand {
            parent: "split".into(),
            subcommand_prefix: ":".into(),
        }
    );
    assert_eq!(pc.prefix, "l");
}

#[test]
fn test_parse_context_vsplit_colon_subcommand() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("vsplit :fr", &reg, &doc_reg);
    assert_eq!(
        pc.context,
        CompletionContext::Subcommand {
            parent: "vsplit".into(),
            subcommand_prefix: ":".into(),
        }
    );
    assert_eq!(pc.prefix, "fr");
}

#[test]
fn test_parse_context_split_filepath() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("split src/f", &reg, &doc_reg);
    assert!(matches!(pc.context, CompletionContext::FilePath { .. }));
}

#[test]
fn test_parse_context_split_empty_is_filepath() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("split ", &reg, &doc_reg);
    assert!(matches!(pc.context, CompletionContext::FilePath { .. }));
}

#[test]
fn test_complete_split_subcommands() {
    let result = complete_subcommand("split", "");
    let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(names.contains(&"left"));
    assert!(names.contains(&"right"));
    assert!(names.contains(&"resize"));
}

#[test]
fn test_complete_subcommand_no_same_first_letter_alias() {
    let result = complete_subcommand("split", "");
    let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(
        !names.contains(&"l"),
        "alias 'l' same first letter as 'left'"
    );
    assert!(names.contains(&"left"));
}

#[test]
fn test_complete_buffer_no_ls_alias() {
    let result = complete_subcommand("buffer", "");
    let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(names.contains(&"list"));
    assert!(
        !names.contains(&"ls"),
        "alias 'ls' same first letter as 'list'"
    );
}

#[test]
fn test_complete_vsplit_subcommands() {
    let result = complete_subcommand("vsplit", "");
    let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(names.contains(&"down"));
}

#[test]
fn test_parse_context_token_start() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("edit foo.txt", &reg, &doc_reg);
    assert_eq!(pc.token_start, 5);

    let pc = parse_context("set ", &reg, &doc_reg);
    assert_eq!(pc.token_start, 4);

    let pc = parse_context("quit", &reg, &doc_reg);
    assert_eq!(pc.token_start, 0);
}

#[test]
fn test_resolve_completion_stale() {
    let result = CompletionResult {
        common_prefix: "quit".into(),
        candidates: vec![CompletionCandidate {
            text: "quit".into(),
            description: String::new(),
            is_directory: false,
        }],
    };
    let action = resolve_completion(result, "qu", 0, "qui", false);
    assert!(matches!(action, CompletionAction::Discard));
}

#[test]
fn test_resolve_completion_single() {
    let result = CompletionResult {
        common_prefix: "quit".into(),
        candidates: vec![CompletionCandidate {
            text: "quit".into(),
            description: String::new(),
            is_directory: false,
        }],
    };
    let action = resolve_completion(result, "qu", 0, "qu", false);
    assert!(matches!(action, CompletionAction::ApplyAndClear { .. }));
}

#[test]
fn test_resolve_completion_dropdown() {
    let result = CompletionResult {
        common_prefix: "w".into(),
        candidates: vec![
            CompletionCandidate {
                text: "write".into(),
                description: String::new(),
                is_directory: false,
            },
            CompletionCandidate {
                text: "wq".into(),
                description: String::new(),
                is_directory: false,
            },
        ],
    };
    let action = resolve_completion(result, "w", 0, "w", false);
    assert!(matches!(action, CompletionAction::ShowDropdown { .. }));
}

#[test]
fn test_from_candidates_sorts_dirs_first() {
    let candidates = vec![
        CompletionCandidate {
            text: "file.rs".into(),
            description: String::new(),
            is_directory: false,
        },
        CompletionCandidate {
            text: "src/".into(),
            description: "directory".into(),
            is_directory: true,
        },
    ];
    let result = CompletionResult::from_candidates(candidates);
    assert_eq!(result.candidates[0].text, "src/");
    assert_eq!(result.candidates[1].text, "file.rs");
}

#[test]
fn test_parse_context_local_setting_name_prefix() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("setlocal et", &reg, &doc_reg);
    assert_eq!(pc.context, CompletionContext::LocalSettingName);
    assert_eq!(pc.prefix, "et");
}

#[test]
fn test_parse_context_local_setting_name_empty() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("setlocal ", &reg, &doc_reg);
    assert_eq!(pc.context, CompletionContext::LocalSettingName);
    assert_eq!(pc.prefix, "");
}

#[test]
fn test_parse_context_local_setting_value_exact_name() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("setlocal expandtabs ", &reg, &doc_reg);
    assert_eq!(
        pc.context,
        CompletionContext::LocalSettingValue {
            name: "expandtabs".into()
        }
    );
}

#[test]
fn test_parse_context_local_setting_value_alias_resolved() {
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("setlocal et ", &reg, &doc_reg);
    assert_eq!(
        pc.context,
        CompletionContext::LocalSettingValue {
            name: "expandtabs".into()
        }
    );
}

#[test]
fn test_parse_context_setl_alias_resolves_to_local_setting_name() {
    // "setl" is an alias for "setlocal"
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("setl ", &reg, &doc_reg);
    assert_eq!(pc.context, CompletionContext::LocalSettingName);
    assert_eq!(pc.prefix, "");
}

#[test]
fn test_parse_context_set_setting_value_alias_resolved() {
    // "clborderstyle" is an alias for "command_line.borderstyle"
    let reg = create_settings_registry();
    let doc_reg = create_document_settings_registry();
    let pc = parse_context("set clborderstyle ", &reg, &doc_reg);
    assert_eq!(
        pc.context,
        CompletionContext::SettingValue {
            name: "command_line.borderstyle".into()
        }
    );
}

#[test]
fn test_complete_local_setting_name_returns_doc_settings() {
    let doc_reg = create_document_settings_registry();
    let result = complete_setting_name("", &doc_reg);
    let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"expandtabs"), "expandtabs must appear");
    assert!(texts.contains(&"tabwidth"), "tabwidth must appear");
    assert!(texts.contains(&"line_ending"), "line_ending must appear");
}

#[test]
fn test_complete_local_setting_name_does_not_show_global_settings() {
    let doc_reg = create_document_settings_registry();
    let result = complete_setting_name("", &doc_reg);
    let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    // "number" is a valid document-local setting (setlocal number/nonumber)
    assert!(texts.contains(&"number"), "local 'number' must appear");
    assert!(
        !texts.contains(&"appearance.background"),
        "global color setting must not appear"
    );
}

#[test]
fn test_complete_global_setting_name_does_not_show_local_settings() {
    let reg = create_settings_registry();
    let result = complete_setting_name("", &reg);
    let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(
        !texts.contains(&"expandtabs"),
        "local 'expandtabs' must not appear"
    );
    assert!(
        !texts.contains(&"tabwidth"),
        "local 'tabwidth' must not appear"
    );
}

#[test]
fn test_complete_setting_value_integer_shows_current_value() {
    let doc_reg = create_document_settings_registry();
    let opts = DocumentOptions::default(); // tab_width = 4
    let result = complete_setting_value("tabwidth", &doc_reg, Some(&opts));
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].text, "4");
}

#[test]
fn test_complete_setting_value_float_shows_current_value() {
    let reg = create_settings_registry();
    let settings = UserSettings::new();
    // command_line.width_ratio defaults to 0.6
    let result = complete_setting_value("command_line.width_ratio", &reg, Some(&settings));
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].text, "0.6");
}

#[test]
fn test_complete_setting_value_integer_shows_current_value_global() {
    let reg = create_settings_registry();
    let settings = UserSettings::new();
    // command_line.height defaults to 3
    let result = complete_setting_value("command_line.height", &reg, Some(&settings));
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].text, "3");
}

#[test]
fn test_complete_setting_value_boolean_still_shows_both_options() {
    let doc_reg = create_document_settings_registry();
    let opts = DocumentOptions::default();
    let result = complete_setting_value("expandtabs", &doc_reg, Some(&opts));
    assert_eq!(result.candidates.len(), 2);
    assert!(result.candidates.iter().any(|c| c.text == "true"));
    assert!(result.candidates.iter().any(|c| c.text == "false"));
}

#[test]
fn test_complete_setting_value_enum_still_shows_all_variants() {
    let doc_reg = create_document_settings_registry();
    let opts = DocumentOptions::default();
    let result = complete_setting_value("line_ending", &doc_reg, Some(&opts));
    assert!(result.candidates.iter().any(|c| c.text == "lf"));
    assert!(result.candidates.iter().any(|c| c.text == "crlf"));
}

#[test]
fn test_complete_setting_value_integer_no_current_is_empty() {
    let doc_reg = create_document_settings_registry();
    let result = complete_setting_value::<DocumentOptions>("tabwidth", &doc_reg, None);
    assert!(result.candidates.is_empty());
}

#[test]
fn test_complete_setting_value_color_no_current_is_empty() {
    let reg = create_settings_registry();
    let result = complete_setting_value::<UserSettings>("appearance.background", &reg, None);
    assert!(result.candidates.is_empty());
}

#[test]
fn test_complete_setting_value_color_shows_current_value() {
    let reg = create_settings_registry();
    let mut settings = UserSettings::new();
    settings.editor_bg = None;
    let result = complete_setting_value("appearance.background", &reg, Some(&settings));
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].text, "none");
}
