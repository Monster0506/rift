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
