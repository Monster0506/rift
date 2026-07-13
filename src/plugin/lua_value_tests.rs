use super::*;
use crate::annotations::Value;

#[test]
fn lua_table_round_trips_through_value() {
    let lua = mlua::Lua::new();
    let tbl: mlua::Table = lua
        .load(r#"return { checked = false, label = "go", n = 3, f = 1.5, items = {1,2,3} }"#)
        .eval()
        .unwrap();
    let v = value_from_lua_table(&tbl).unwrap();
    assert_eq!(v.get("checked"), Some(&Value::Bool(false)));
    assert_eq!(v.get("label"), Some(&Value::Str("go".into())));
    assert_eq!(v.get("n"), Some(&Value::Int(3)));
    assert_eq!(v.get("f"), Some(&Value::Float(1.5)));
    assert_eq!(
        v.get("items"),
        Some(&Value::List(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3)
        ]))
    );
}

#[test]
fn lua_array_table_becomes_list() {
    let lua = mlua::Lua::new();
    let tbl: mlua::Table = lua.load(r#"return {10, 20, 30}"#).eval().unwrap();
    let v = value_from_lua_table(&tbl).unwrap();
    assert_eq!(
        v,
        Value::List(vec![Value::Int(10), Value::Int(20), Value::Int(30)])
    );
}

#[test]
fn value_to_lua_and_back_is_lossless() {
    let lua = mlua::Lua::new();
    let v = Value::Map(vec![
        ("a".into(), Value::Int(1)),
        (
            "b".into(),
            Value::List(vec![Value::Str("x".into()), Value::Bool(true)]),
        ),
    ]);
    let lua_val = value_into_lua(v.clone(), &lua).unwrap();
    let tbl = match lua_val {
        mlua::Value::Table(t) => t,
        _ => panic!("expected table"),
    };
    assert_eq!(value_from_lua_table(&tbl).unwrap(), v);
}

#[test]
fn lua_nil_becomes_null() {
    assert_eq!(value_from_lua(&mlua::Value::Nil).unwrap(), Value::Null);
}
