use super::*;

#[test]
fn value_serde_json_round_trips_all_variants() {
    let v = Value::Map(vec![
        ("null".into(), Value::Null),
        ("b".into(), Value::Bool(true)),
        ("i".into(), Value::Int(-7)),
        ("f".into(), Value::Float(1.5)),
        ("s".into(), Value::Str("héllo".into())),
        (
            "list".into(),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        ),
    ]);
    let json = serde_json::to_string(&v).unwrap();
    let back: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v, back);
}

#[test]
fn value_map_serializes_as_json_object_in_order() {
    let v = Value::Map(vec![
        ("z".into(), Value::Int(1)),
        ("a".into(), Value::Int(2)),
    ]);
    assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"z":1,"a":2}"#);
}

#[test]
fn value_list_serializes_as_json_array() {
    let v = Value::List(vec![Value::Int(1), Value::Str("x".into())]);
    assert_eq!(serde_json::to_string(&v).unwrap(), r#"[1,"x"]"#);
}

#[test]
fn value_int_and_float_stay_distinct_through_json() {
    let i: Value = serde_json::from_str("3").unwrap();
    let f: Value = serde_json::from_str("3.5").unwrap();
    assert_eq!(i, Value::Int(3));
    assert_eq!(f, Value::Float(3.5));
    // A whole-valued float keeps its decimal and round-trips back to Float.
    let whole = Value::Float(2.0);
    let back: Value = serde_json::from_str(&serde_json::to_string(&whole).unwrap()).unwrap();
    assert_eq!(back, Value::Float(2.0));
}

#[test]
fn value_get_and_set_on_map() {
    let mut v = Value::map();
    v.set("checked", Value::Bool(false));
    assert_eq!(v.get("checked"), Some(&Value::Bool(false)));
    v.set("checked", Value::Bool(true));
    assert_eq!(v.get("checked"), Some(&Value::Bool(true)));
    assert_eq!(v.get("missing"), None);
}

#[test]
fn lua_table_round_trips_through_value() {
    let lua = mlua::Lua::new();
    let tbl: mlua::Table = lua
        .load(r#"return { checked = false, label = "go", n = 3, f = 1.5, items = {1,2,3} }"#)
        .eval()
        .unwrap();
    let v = Value::from_lua_table(&tbl).unwrap();
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
    let v = Value::from_lua_table(&tbl).unwrap();
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
    let lua_val = v.clone().into_lua(&lua).unwrap();
    let tbl = match lua_val {
        mlua::Value::Table(t) => t,
        _ => panic!("expected table"),
    };
    assert_eq!(Value::from_lua_table(&tbl).unwrap(), v);
}

#[test]
fn lua_nil_becomes_null() {
    assert_eq!(Value::from_lua(&mlua::Value::Nil).unwrap(), Value::Null);
}
