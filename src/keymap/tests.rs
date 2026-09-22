use super::*;
use crate::action::{EditorAction, Motion};
use crate::keymap::defaults::register_defaults;

#[test]
fn test_register_and_get() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
    assert_eq!(map.get_action(KeyContext::Global, Key::Char('k')), None);
}

#[test]
fn test_sequence() {
    let mut map = KeyMap::new();
    map.register_sequence(
        KeyContext::Global,
        vec![Key::Char('d'), Key::Char('d')],
        Action::Editor(EditorAction::DeleteLine),
    );

    // Partial match
    assert_eq!(
        map.lookup(KeyContext::Global, &[Key::Char('d')]),
        MatchResult::Prefix
    );

    // Exact match
    assert_eq!(
        map.lookup(KeyContext::Global, &[Key::Char('d'), Key::Char('d')]),
        MatchResult::Exact(&Action::Editor(EditorAction::DeleteLine))
    );

    // No match
    assert_eq!(
        map.lookup(KeyContext::Global, &[Key::Char('x')]),
        MatchResult::None
    );
}

#[test]
fn test_context_fallback() {
    let mut map = KeyMap::new();
    // Global binding
    map.register(
        KeyContext::Global,
        Key::Char('q'),
        Action::Editor(EditorAction::Quit),
    );

    // Specific binding
    map.register(
        KeyContext::Visual,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    // Test specific context finding specific binding
    assert_eq!(
        map.get_action(KeyContext::Visual, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );

    // Test specific context falling back to global
    assert_eq!(
        map.get_action(KeyContext::Visual, Key::Char('q')),
        Some(&Action::Editor(EditorAction::Quit))
    );

    // Test global context finding global binding
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('q')),
        Some(&Action::Editor(EditorAction::Quit))
    );

    // Test global context NOT finding specific binding
    assert_eq!(map.get_action(KeyContext::Global, Key::Char('j')), None);
}

#[test]
fn test_overwrite() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );

    map.register(KeyContext::Global, Key::Char('j'), Action::Noop);
    assert_eq!(
        map.get_action(KeyContext::Global, Key::Char('j')),
        Some(&Action::Noop)
    );
}

// Buffer -> Normal -> Global fallback chain via resolver

#[test]
fn test_file_explorer_buffer_falls_back_to_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    let ctx = KeyContext::Buffer(BufferKindId::DIRECTORY);
    assert_eq!(
        map.get_action_with_parent(ctx, Key::Char('j'), |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
}

#[test]
fn test_file_explorer_buffer_falls_back_to_global_via_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('q'),
        Action::Editor(EditorAction::Quit),
    );

    let ctx = KeyContext::Buffer(BufferKindId::DIRECTORY);
    assert_eq!(
        map.get_action_with_parent(ctx, Key::Char('q'), |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        Some(&Action::Editor(EditorAction::Quit))
    );
}

#[test]
fn test_file_explorer_buffer_override_shadows_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Char('k'),
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    let ctx = KeyContext::Buffer(BufferKindId::DIRECTORY);
    map.register(
        ctx,
        Key::Char('k'),
        Action::Editor(EditorAction::Move(Motion::PageUp)),
    );

    assert_eq!(
        map.get_action_with_parent(ctx, Key::Char('k'), |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        Some(&Action::Editor(EditorAction::Move(Motion::PageUp)))
    );
    assert_eq!(
        map.get_action(KeyContext::Normal, Key::Char('k')),
        Some(&Action::Editor(EditorAction::Move(Motion::Up)))
    );
}

#[test]
fn test_undotree_falls_back_to_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    let ctx = KeyContext::Buffer(BufferKindId::UNDO_TREE);
    assert_eq!(
        map.get_action_with_parent(ctx, Key::Char('j'), |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
}

#[test]
fn test_undotree_falls_back_to_global_via_normal() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Escape,
        Action::Editor(EditorAction::EnterNormalMode),
    );

    let ctx = KeyContext::Buffer(BufferKindId::UNDO_TREE);
    assert_eq!(
        map.get_action_with_parent(ctx, Key::Escape, |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        Some(&Action::Editor(EditorAction::EnterNormalMode))
    );
}

#[test]
fn test_undotree_override_shadows_normal() {
    let mut map = KeyMap::new();
    map.register(KeyContext::Normal, Key::Enter, Action::Noop);
    let ctx = KeyContext::Buffer(BufferKindId::UNDO_TREE);
    map.register(
        ctx,
        Key::Enter,
        Action::Buffer("undotree:select".to_string()),
    );

    assert_eq!(
        map.get_action_with_parent(ctx, Key::Enter, |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        Some(&Action::Buffer("undotree:select".to_string()))
    );
}

#[test]
fn test_normal_does_not_see_file_explorer_buffer_bindings() {
    let mut map = KeyMap::new();
    let ctx = KeyContext::Buffer(BufferKindId::DIRECTORY);
    map.register(
        ctx,
        Key::Enter,
        Action::Buffer("explorer:select".to_string()),
    );

    assert_eq!(map.get_action(KeyContext::Normal, Key::Enter), None);
}

#[test]
fn test_normal_does_not_see_undotree_bindings() {
    let mut map = KeyMap::new();
    let ctx = KeyContext::Buffer(BufferKindId::UNDO_TREE);
    map.register(
        ctx,
        Key::Enter,
        Action::Buffer("undotree:select".to_string()),
    );

    assert_eq!(map.get_action(KeyContext::Normal, Key::Enter), None);
}

#[test]
fn test_file_explorer_buffer_all_normal_motions_accessible() {
    let mut map = KeyMap::new();
    let motions = vec![
        (Key::Char('h'), Motion::Left),
        (Key::Char('j'), Motion::Down),
        (Key::Char('k'), Motion::Up),
        (Key::Char('l'), Motion::Right),
    ];
    for (key, motion) in &motions {
        map.register(
            KeyContext::Normal,
            key.clone(),
            Action::Editor(EditorAction::Move(*motion)),
        );
    }

    let ctx = KeyContext::Buffer(BufferKindId::DIRECTORY);
    for (key, motion) in &motions {
        assert_eq!(
            map.get_action_with_parent(ctx, key.clone(), |c| (c == ctx)
                .then_some(KeyContext::Normal)),
            Some(&Action::Editor(EditorAction::Move(*motion))),
            "Directory should inherit {:?} from Normal",
            key
        );
    }
}

#[test]
fn test_undotree_all_normal_motions_accessible() {
    let mut map = KeyMap::new();
    let motions = vec![
        (Key::Char('h'), Motion::Left),
        (Key::Char('j'), Motion::Down),
        (Key::Char('k'), Motion::Up),
        (Key::Char('l'), Motion::Right),
    ];
    for (key, motion) in &motions {
        map.register(
            KeyContext::Normal,
            key.clone(),
            Action::Editor(EditorAction::Move(*motion)),
        );
    }

    let ctx = KeyContext::Buffer(BufferKindId::UNDO_TREE);
    for (key, motion) in &motions {
        assert_eq!(
            map.get_action_with_parent(ctx, key.clone(), |c| (c == ctx)
                .then_some(KeyContext::Normal)),
            Some(&Action::Editor(EditorAction::Move(*motion))),
            "UndoTree should inherit {:?} from Normal",
            key
        );
    }
}

#[test]
fn test_sequence_fallback_through_normal_to_global() {
    let mut map = KeyMap::new();
    map.register_sequence(
        KeyContext::Global,
        vec![Key::Char('g'), Key::Char('g')],
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );

    let ctx = KeyContext::Buffer(BufferKindId::DIRECTORY);
    assert_eq!(
        map.lookup_with_parent(ctx, &[Key::Char('g'), Key::Char('g')], |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        MatchResult::Exact(&Action::Editor(EditorAction::Move(Motion::StartOfFile)))
    );
}

#[test]
fn test_sequence_fallback_undotree_through_normal_to_global() {
    let mut map = KeyMap::new();
    map.register_sequence(
        KeyContext::Global,
        vec![Key::Char('g'), Key::Char('g')],
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );

    let ctx = KeyContext::Buffer(BufferKindId::UNDO_TREE);
    assert_eq!(
        map.lookup_with_parent(ctx, &[Key::Char('g'), Key::Char('g')], |c| (c == ctx)
            .then_some(KeyContext::Normal)),
        MatchResult::Exact(&Action::Editor(EditorAction::Move(Motion::StartOfFile)))
    );
}

#[test]
fn test_default_explorer_toggle_hidden_keybind() {
    let mut map = KeyMap::new();
    register_defaults(&mut map);

    assert_eq!(
        map.get_action(KeyContext::Buffer(BufferKindId::DIRECTORY), Key::Char('H')),
        Some(&Action::Editor(EditorAction::ExplorerToggleHidden)),
        "Directory 'H' should be bound to ExplorerToggleHidden by default"
    );
}

#[test]
fn test_default_word_end_keybind() {
    let mut map = KeyMap::new();
    register_defaults(&mut map);

    assert_eq!(
        map.get_action(KeyContext::Normal, Key::Char('e')),
        Some(&Action::Editor(EditorAction::Move(Motion::WordEnd))),
        "Normal 'e' should be bound to Motion::WordEnd by default"
    );
}

#[test]
fn test_operator_pending_text_objects_not_in_trie() {
    use crate::keymap::trie::MatchResult;

    // Text objects are intercepted before trie lookup, so 'i' has no trie entry.
    let mut map = KeyMap::new();
    register_defaults(&mut map);

    assert_eq!(
        map.lookup(KeyContext::OperatorPending, &[Key::Char('i')]),
        map.lookup(KeyContext::Normal, &[Key::Char('i')]),
        "OperatorPending should fall through to Normal for 'i' (no trie text-object entry)"
    );

    // OperatorPending falls through to Normal for regular motions.
    assert!(
        matches!(
            map.lookup(KeyContext::OperatorPending, &[Key::Char('h')]),
            MatchResult::Exact(_)
        ),
        "'h' should be reachable in OperatorPending via Normal fallthrough"
    );
}

#[test]
fn test_ctrl_w_ctrl_hjkl_match_ctrl_w_hjkl() {
    let mut map = KeyMap::new();
    register_defaults(&mut map);
    let ww = Key::Ctrl(b'w');

    for ch in [b'h', b'j', b'k', b'l'] {
        let plain = map.lookup(KeyContext::Normal, &[ww.clone(), Key::Char(ch as char)]);
        let ctrl = map.lookup(KeyContext::Normal, &[ww.clone(), Key::Ctrl(ch)]);

        assert!(
            matches!(plain, MatchResult::Exact(_)),
            "<C-w>{} should be bound by default",
            ch as char
        );
        assert_eq!(
            plain, ctrl,
            "<C-w><C-{0}> should resolve to the same action as <C-w>{0}",
            ch as char
        );
    }
}

#[test]
fn test_ctrl_w_ctrl_shift_hjkl_match_ctrl_w_hjkl() {
    // Backends can emit CtrlShift for fast Ctrl-W chords.
    // The alias keeps Ctrl-W H/J/K/L navigation available.
    let mut map = KeyMap::new();
    register_defaults(&mut map);
    let ww = Key::Ctrl(b'w');

    for ch in [b'h', b'j', b'k', b'l'] {
        let plain = map.lookup(KeyContext::Normal, &[ww.clone(), Key::Char(ch as char)]);
        let ctrl_shift = map.lookup(KeyContext::Normal, &[ww.clone(), Key::CtrlShift(ch)]);

        assert_eq!(
            plain, ctrl_shift,
            "<C-w><C-S-{0}> should resolve to the same action as <C-w>{0}",
            ch as char
        );
    }
}

#[test]
fn test_ctrl_w_ctrl_h_is_a_prefix_after_just_ctrl_w() {
    let mut map = KeyMap::new();
    register_defaults(&mut map);

    assert_eq!(
        map.lookup(KeyContext::Normal, &[Key::Ctrl(b'w')]),
        MatchResult::Prefix,
        "<C-w> alone should still be a pending prefix, not swallowed by the <C-h/j/k/l> aliases"
    );
}

#[test]
fn test_buffer_context_custom_parent_resolution() {
    let mut map = KeyMap::new();
    let kind_id = BufferKindId::new(std::num::NonZeroU32::new(100).unwrap());
    let buffer_ctx = KeyContext::Buffer(kind_id);

    // Base normal bindings
    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    map.register(
        KeyContext::Normal,
        Key::Char('q'),
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    // Global binding
    map.register(
        KeyContext::Global,
        Key::Char('x'),
        Action::Editor(EditorAction::Quit),
    );
    // Buffer-specific override
    map.register(
        buffer_ctx,
        Key::Char('q'),
        Action::Buffer("buffer:quit".to_string()),
    );

    let resolver = |ctx: KeyContext| match ctx {
        KeyContext::Buffer(id) if id == kind_id => Some(KeyContext::Normal),
        _ => None,
    };

    // Buffer local binding takes precedence
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('q'), resolver),
        Some(&Action::Buffer("buffer:quit".to_string()))
    );

    // Buffer falls back to Normal for 'j'
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('j'), resolver),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );

    // Buffer falls back through Normal to Global for 'x'
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('x'), resolver),
        Some(&Action::Editor(EditorAction::Quit))
    );

    // Unbound key returns None
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('z'), resolver),
        None
    );
}

#[test]
fn test_buffer_context_no_fallback() {
    let mut map = KeyMap::new();
    let kind_id = BufferKindId::new(std::num::NonZeroU32::new(101).unwrap());
    let buffer_ctx = KeyContext::Buffer(kind_id);

    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    map.register(
        buffer_ctx,
        Key::Char('q'),
        Action::Buffer("buffer:quit".to_string()),
    );

    // Resolver returning None for buffer context (KeyFallback::None policy)
    let no_fallback = |_ctx: KeyContext| None;

    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('q'), no_fallback),
        Some(&Action::Buffer("buffer:quit".to_string()))
    );
    // 'j' exists in Normal, but buffer has no fallback so it must resolve to None
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('j'), no_fallback),
        None
    );
}

#[test]
fn test_buffer_context_global_fallback() {
    let mut map = KeyMap::new();
    let kind_id = BufferKindId::new(std::num::NonZeroU32::new(102).unwrap());
    let buffer_ctx = KeyContext::Buffer(kind_id);

    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    map.register(
        KeyContext::Global,
        Key::Char('x'),
        Action::Editor(EditorAction::Quit),
    );

    // Resolver maps Buffer directly to Global (KeyFallback::Global policy)
    let global_fallback = |ctx: KeyContext| match ctx {
        KeyContext::Buffer(id) if id == kind_id => Some(KeyContext::Global),
        _ => None,
    };

    // Global binding accessible
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('x'), global_fallback),
        Some(&Action::Editor(EditorAction::Quit))
    );
    // Normal binding skipped
    assert_eq!(
        map.get_action_with_parent(buffer_ctx, Key::Char('j'), global_fallback),
        None
    );
}

#[test]
fn test_static_contexts_retain_current_fallback() {
    let mut map = KeyMap::new();
    map.register(
        KeyContext::Global,
        Key::Char('q'),
        Action::Editor(EditorAction::Quit),
    );
    map.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    // Default lookup preserves Visual -> Normal -> Global
    assert_eq!(
        map.get_action(KeyContext::Visual, Key::Char('j')),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
    assert_eq!(
        map.get_action(KeyContext::Visual, Key::Char('q')),
        Some(&Action::Editor(EditorAction::Quit))
    );

    // Custom resolver only handling Buffer still preserves static fallback
    let buffer_only_resolver = |_ctx: KeyContext| None;
    assert_eq!(
        map.get_action_with_parent(KeyContext::Visual, Key::Char('j'), buffer_only_resolver),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
    assert_eq!(
        map.get_action_with_parent(KeyContext::Visual, Key::Char('q'), buffer_only_resolver),
        Some(&Action::Editor(EditorAction::Quit))
    );
}

#[test]
fn test_layered_registration_and_restoration_by_sequence() {
    let mut map = KeyMap::new();
    let ctx = KeyContext::Normal;
    let key = Key::Char('j');

    // Layer 1
    map.register(
        ctx,
        key.clone(),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    assert_eq!(
        map.get_action(ctx, key.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
    assert_eq!(map.layer_count(ctx, std::slice::from_ref(&key)), 1);

    // Layer 2 (overrides layer 1)
    map.register(
        ctx,
        key.clone(),
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    assert_eq!(
        map.get_action(ctx, key.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Up)))
    );
    assert_eq!(map.layer_count(ctx, std::slice::from_ref(&key)), 2);

    // Unregister layer 2 -> restores layer 1
    assert!(map.unregister_sequence(ctx, std::slice::from_ref(&key)));
    assert_eq!(
        map.get_action(ctx, key.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
    assert_eq!(map.layer_count(ctx, std::slice::from_ref(&key)), 1);

    // Unregister layer 1 -> sequence is completely removed
    assert!(map.unregister_sequence(ctx, std::slice::from_ref(&key)));
    assert_eq!(map.get_action(ctx, key.clone()), None);
    assert_eq!(map.layer_count(ctx, std::slice::from_ref(&key)), 0);

    // Unregister again returns false
    assert!(!map.unregister_sequence(ctx, std::slice::from_ref(&key)));
}

#[test]
fn test_layered_registration_and_restoration_by_token() {
    let mut map = KeyMap::new();
    let ctx = KeyContext::Global;
    let keys = vec![Key::Ctrl(b'x')];

    let token1 = KeyBindingToken::new(10);
    let token2 = KeyBindingToken::new(20);

    map.register_with_token(
        ctx,
        keys.clone(),
        Action::Editor(EditorAction::Quit),
        token1,
    );
    assert_eq!(
        map.lookup(ctx, &keys),
        MatchResult::Exact(&Action::Editor(EditorAction::Quit))
    );

    // Layer 2 with token 20
    map.register_with_token(
        ctx,
        keys.clone(),
        Action::Editor(EditorAction::EnterNormalMode),
        token2,
    );
    assert_eq!(
        map.lookup(ctx, &keys),
        MatchResult::Exact(&Action::Editor(EditorAction::EnterNormalMode))
    );

    // Removing token2 restores token1
    assert!(map.unregister_token(token2));
    assert_eq!(
        map.lookup(ctx, &keys),
        MatchResult::Exact(&Action::Editor(EditorAction::Quit))
    );

    // Removing token1 removes binding completely
    assert!(map.unregister_token(token1));
    assert_eq!(map.lookup(ctx, &keys), MatchResult::None);
}

#[test]
fn test_layered_registration_and_restoration_by_owner() {
    let mut map = KeyMap::new();
    let ctx = KeyContext::Normal;
    let key_x = Key::Char('x');
    let key_y = Key::Char('y');

    // Base layer (no owner)
    map.register(
        ctx,
        key_x.clone(),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );

    // Plugin owner 1 registers key_x override and key_y
    let plugin_owner = KeyBindingToken::new(1001);
    map.register_with_owner(
        ctx,
        vec![key_x.clone()],
        Action::Editor(EditorAction::Move(Motion::Up)),
        plugin_owner,
    );
    map.register_with_owner(
        ctx,
        vec![key_y.clone()],
        Action::Editor(EditorAction::Move(Motion::Left)),
        plugin_owner,
    );

    // User owner 2 registers a newer key_x override
    let user_owner = KeyBindingToken::new(2002);
    map.register_with_owner(
        ctx,
        vec![key_x.clone()],
        Action::Editor(EditorAction::Move(Motion::Right)),
        user_owner,
    );

    // Active action for x is user override (Right)
    assert_eq!(
        map.get_action(ctx, key_x.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Right)))
    );
    assert_eq!(
        map.get_action(ctx, key_y.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Left)))
    );

    // Retiring plugin owner 1 removes key_y and its layer on key_x
    let removed_count = map.unregister_owner(plugin_owner);
    assert_eq!(removed_count, 2);

    // key_y is gone
    assert_eq!(map.get_action(ctx, key_y.clone()), None);

    // key_x still has user override active
    assert_eq!(
        map.get_action(ctx, key_x.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Right)))
    );

    // Retiring user owner 2 restores the original base layer (Down)
    map.unregister_owner(user_owner);
    assert_eq!(
        map.get_action(ctx, key_x.clone()),
        Some(&Action::Editor(EditorAction::Move(Motion::Down)))
    );
}

#[test]
fn test_multi_key_sequence_layer_restoration() {
    let mut map = KeyMap::new();
    let ctx = KeyContext::Normal;
    let seq = vec![Key::Char('g'), Key::Char('d')];

    let token1 = map.register_sequence(
        ctx,
        seq.clone(),
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );
    let token2 = map.register_sequence(
        ctx,
        seq.clone(),
        Action::Buffer("custom:definition".to_string()),
    );

    assert_eq!(
        map.lookup(ctx, &seq),
        MatchResult::Exact(&Action::Buffer("custom:definition".to_string()))
    );
    assert_eq!(map.lookup(ctx, &[Key::Char('g')]), MatchResult::Prefix);

    // Remove top layer
    assert!(map.unregister_token(token2));
    assert_eq!(
        map.lookup(ctx, &seq),
        MatchResult::Exact(&Action::Editor(EditorAction::Move(Motion::StartOfFile)))
    );
    assert_eq!(map.lookup(ctx, &[Key::Char('g')]), MatchResult::Prefix);

    // Remove base layer
    assert!(map.unregister_token(token1));
    assert_eq!(map.lookup(ctx, &seq), MatchResult::None);
    assert_eq!(map.lookup(ctx, &[Key::Char('g')]), MatchResult::None);
}

#[test]
fn test_cyclic_parent_resolution_terminates() {
    let map = KeyMap::new();
    let kind_a = BufferKindId::new(std::num::NonZeroU32::new(1).unwrap());
    let kind_b = BufferKindId::new(std::num::NonZeroU32::new(2).unwrap());

    // Resolver creates a cycle: A -> B -> A
    let cyclic_resolver = |ctx: KeyContext| match ctx {
        KeyContext::Buffer(id) if id == kind_a => Some(KeyContext::Buffer(kind_b)),
        KeyContext::Buffer(id) if id == kind_b => Some(KeyContext::Buffer(kind_a)),
        _ => None,
    };

    assert_eq!(
        map.lookup_with_parent(
            KeyContext::Buffer(kind_a),
            &[Key::Char('z')],
            cyclic_resolver
        ),
        MatchResult::None
    );
}
