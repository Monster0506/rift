use super::SettingsResolver;
use crate::command_line::settings::{create_settings_registry, SettingValue};
use crate::document::definitions::{create_document_settings_registry, DocumentOptions};
use crate::state::UserSettings;

fn resolver() -> SettingsResolver<DocumentOptions, UserSettings> {
    SettingsResolver::new(
        create_document_settings_registry(),
        create_settings_registry(),
    )
}

#[test]
fn set_prefers_local_registry_when_name_exists_there() {
    let r = resolver();
    let mut doc = DocumentOptions::default();
    let mut user = UserSettings::new();
    let mut errors = Vec::new();
    let mut error_handler = |e| errors.push(e);

    r.set(
        "tabwidth",
        Some("8".to_string()),
        &mut doc,
        &mut user,
        &mut error_handler,
    );

    assert!(errors.is_empty());
    assert_eq!(doc.tab_width, 8);
}

#[test]
fn set_falls_back_to_global_registry_when_name_is_not_local() {
    let r = resolver();
    let mut doc = DocumentOptions::default();
    let mut user = UserSettings::new();
    let mut errors = Vec::new();
    let mut error_handler = |e| errors.push(e);

    r.set(
        "ghostcut",
        Some("false".to_string()),
        &mut doc,
        &mut user,
        &mut error_handler,
    );

    assert!(errors.is_empty());
    assert!(!user.ghost_cut);
}

#[test]
fn get_prefers_local_registry_when_name_exists_there() {
    let r = resolver();
    let doc = DocumentOptions {
        tab_width: 8,
        ..Default::default()
    };
    let user = UserSettings::new();

    assert_eq!(
        r.get("tabwidth", &doc, &user),
        Some(SettingValue::Integer(8))
    );
}

#[test]
fn get_falls_back_to_global_registry_when_name_is_not_local() {
    let r = resolver();
    let doc = DocumentOptions::default();
    let mut user = UserSettings::new();
    user.clipboard_ring_size = 42;

    assert_eq!(
        r.get("clipboard.size", &doc, &user),
        Some(SettingValue::Integer(42))
    );
}

#[test]
fn get_returns_none_for_unknown_name() {
    let r = resolver();
    let doc = DocumentOptions::default();
    let user = UserSettings::new();

    assert_eq!(r.get("not_a_real_setting", &doc, &user), None);
}
