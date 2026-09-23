use super::*;

#[test]
fn resolves_exact_then_prefix_then_global() {
    let mut r = DispatchRegistry::new();
    r.register(
        "lsp.diagnostic",
        "activate",
        Handler::Command("exact".into()),
    );
    r.register("lsp.", "activate", Handler::Command("prefix".into()));
    r.register("*", "activate", Handler::Command("global".into()));

    assert_eq!(
        r.resolve(&Kind::new("lsp.diagnostic"), "activate"),
        Some(&Handler::Command("exact".into()))
    );
    assert_eq!(
        r.resolve(&Kind::new("lsp.hint"), "activate"),
        Some(&Handler::Command("prefix".into()))
    );
    assert_eq!(
        r.resolve(&Kind::new("git.blame"), "activate"),
        Some(&Handler::Command("global".into()))
    );
    assert_eq!(r.resolve(&Kind::new("git.blame"), "toggle"), None);
}

#[test]
fn reregister_replaces_handler() {
    let mut r = DispatchRegistry::new();
    r.register("test.runnable", "run", Handler::Lua);
    r.register("test.runnable", "run", Handler::Command("new".into()));
    assert_eq!(
        r.resolve(&Kind::new("test.runnable"), "run"),
        Some(&Handler::Command("new".into()))
    );
}

#[test]
fn remove_key_drops_dangling_handlers() {
    let mut r = DispatchRegistry::new();
    r.register("plug.x", "run", Handler::Lua);
    r.remove_key("plug.x");
    assert_eq!(r.resolve(&Kind::new("plug.x"), "run"), None);
}

#[test]
fn builtins_preloaded() {
    let r = DispatchRegistry::with_builtins();
    assert_eq!(
        r.resolve(&Kind::new("ui.checkbox"), "toggle"),
        Some(&Handler::Builtin(Builtin::ToggleChecked))
    );
    assert_eq!(
        r.resolve(&Kind::new("ui.link"), "activate"),
        Some(&Handler::Builtin(Builtin::FollowLink))
    );
    // The file explorer routes its "open" through the dispatch registry.
    assert_eq!(
        r.resolve(&Kind::new("fs.entry"), "activate"),
        Some(&Handler::Builtin(Builtin::OpenEntry))
    );
}

#[test]
fn kind_defaults_resolve_exact_then_prefix_then_global() {
    use super::super::presentation::{FaceRef, Presentation};
    let mut r = KindRegistry::new();
    r.set_presentation("lsp.diagnostic", Presentation::with_face(FaceRef::new("a")));
    r.set_presentation("lsp.", Presentation::with_face(FaceRef::new("b")));
    r.set_presentation("*", Presentation::with_face(FaceRef::new("c")));

    let face = |k: &str| {
        r.default_presentation(&Kind::new(k))
            .and_then(|p| p.face.as_ref())
            .map(|f| f.0.clone())
    };
    assert_eq!(face("lsp.diagnostic"), Some("a".into()));
    assert_eq!(face("lsp.hint"), Some("b".into()));
    assert_eq!(face("git.blame"), Some("c".into()));
}

#[test]
fn kind_defaults_description_falls_back() {
    let mut r = KindRegistry::new();
    r.set_description("lsp.", "language server note");
    assert_eq!(
        r.default_description(&Kind::new("lsp.diagnostic")),
        Some("language server note")
    );
    assert_eq!(r.default_description(&Kind::new("git.blame")), None);
}

#[test]
fn core_kind_defaults_preloaded() {
    let r = KindRegistry::with_core();
    // Diagnostics get the error face and a description by default.
    assert!(r
        .default_presentation(&Kind::new("lsp.diagnostic"))
        .is_some());
    assert_eq!(
        r.default_description(&Kind::new("lsp.diagnostic")),
        Some("diagnostic")
    );
    // Links default to underlined; buttons to reverse video.
    let link = r.default_presentation(&Kind::new("ui.link")).unwrap();
    assert!(link.style.as_ref().map(|s| s.underline).unwrap_or(false));
    let button = r.default_presentation(&Kind::new("ui.button")).unwrap();
    assert!(button.style.as_ref().map(|s| s.reverse).unwrap_or(false));
}

#[test]
fn toggle_checked_flips_payload() {
    let mut p = Value::map();
    p.set("checked", Value::Bool(false));
    toggle_checked(&mut p);
    assert_eq!(p.get("checked"), Some(&Value::Bool(true)));
    toggle_checked(&mut p);
    assert_eq!(p.get("checked"), Some(&Value::Bool(false)));
}

#[test]
fn toggle_checked_defaults_missing_to_true() {
    let mut p = Value::map();
    toggle_checked(&mut p);
    assert_eq!(p.get("checked"), Some(&Value::Bool(true)));
}
