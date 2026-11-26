use super::SettingsResolver;
use crate::command_line::settings::descriptor::SettingValue;
use crate::document::definitions::{create_document_settings_registry, DocumentOptions};
use crate::state::definitions::create_settings_registry;
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

    r.set("tabwidth", Some("8"), &mut doc, &mut user).unwrap();
    assert_eq!(doc.tab_width(), 8);
}

#[test]
fn set_falls_back_to_global_registry_when_name_is_not_local() {
    let r = resolver();
    let mut doc = DocumentOptions::default();
    let mut user = UserSettings::new();

    r.set("ghostcut", Some("false"), &mut doc, &mut user)
        .unwrap();
    assert!(!user.editing().ghost_cut);
}

#[test]
fn get_prefers_local_registry_when_name_exists_there() {
    let r = resolver();
    let mut doc = DocumentOptions::default();
    create_document_settings_registry()
        .set("tabwidth", Some("8"), &mut doc)
        .unwrap();
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
    create_settings_registry()
        .set("clipboard.size", Some("42"), &mut user)
        .unwrap();

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
