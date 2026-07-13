//! Lua <-> `annotations::Value` conversions; round-trip is lossless for the supported set.

use crate::annotations::Value;

/// Convert an arbitrary Lua value into a `Value`; `nil` becomes `Null`.
pub(crate) fn value_from_lua(value: &mlua::Value) -> mlua::Result<Value> {
    Ok(match value {
        mlua::Value::Nil => Value::Null,
        mlua::Value::Boolean(b) => Value::Bool(*b),
        mlua::Value::Integer(i) => Value::Int(*i),
        mlua::Value::Number(n) => Value::Float(*n),
        mlua::Value::String(s) => Value::Str(s.to_str()?.to_string()),
        mlua::Value::Table(t) => value_from_lua_table(t)?,
        other => {
            return Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "annotations::Value".to_string(),
                message: Some("unsupported Lua value in annotation payload".to_string()),
            })
        }
    })
}

/// Tables keyed exactly `1..=n` become a `List`; otherwise a `Map` with
/// stringified keys sorted for deterministic ordering.
pub(crate) fn value_from_lua_table(table: &mlua::Table) -> mlua::Result<Value> {
    let len = table.raw_len();
    let mut is_array = len > 0;
    if is_array {
        // Confirm keys are exactly 1..=len with no extra (string) keys.
        let mut count = 0usize;
        for pair in table.clone().pairs::<mlua::Value, mlua::Value>() {
            let (k, _) = pair?;
            match k {
                mlua::Value::Integer(i) if i >= 1 && (i as usize) <= len => count += 1,
                _ => {
                    is_array = false;
                    break;
                }
            }
        }
        if is_array && count != len {
            is_array = false;
        }
    }

    if is_array {
        let mut items = Vec::with_capacity(len);
        for i in 1..=len {
            let v: mlua::Value = table.raw_get(i)?;
            items.push(value_from_lua(&v)?);
        }
        Ok(Value::List(items))
    } else {
        let mut pairs: Vec<(String, Value)> = Vec::new();
        for pair in table.clone().pairs::<mlua::Value, mlua::Value>() {
            let (k, v) = pair?;
            let key = match k {
                mlua::Value::String(s) => s.to_str()?.to_string(),
                mlua::Value::Integer(i) => i.to_string(),
                mlua::Value::Number(n) => n.to_string(),
                other => {
                    return Err(mlua::Error::FromLuaConversionError {
                        from: other.type_name(),
                        to: "annotations::Value map key".to_string(),
                        message: Some("annotation payload keys must be string-like".to_string()),
                    })
                }
            };
            pairs.push((key, value_from_lua(&v)?));
        }
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Value::Map(pairs))
    }
}

/// Convert a `Value` into a Lua value (the inverse of [`value_from_lua`]).
pub(crate) fn value_into_lua(value: Value, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
    Ok(match value {
        Value::Null => mlua::Value::Nil,
        Value::Bool(b) => mlua::Value::Boolean(b),
        Value::Int(i) => mlua::Value::Integer(i),
        Value::Float(f) => mlua::Value::Number(f),
        Value::Str(s) => mlua::Value::String(lua.create_string(&s)?),
        Value::List(items) => {
            let t = lua.create_table()?;
            for (idx, item) in items.into_iter().enumerate() {
                t.raw_set(idx + 1, value_into_lua(item, lua)?)?;
            }
            mlua::Value::Table(t)
        }
        Value::Map(pairs) => {
            let t = lua.create_table()?;
            for (k, v) in pairs {
                t.raw_set(k, value_into_lua(v, lua)?)?;
            }
            mlua::Value::Table(t)
        }
    })
}

#[cfg(test)]
#[path = "lua_value_tests.rs"]
mod lua_value_tests;
