//! Generic serializable annotation payload (design.md sec 3).
//! Maps 1:1 to Lua tables and to JSON; holds no in-process handles.

use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A recursive, serializable value. See the module docs for the design rationale.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    /// Key-ordered map. Order is preserved through serialization for determinism.
    Map(Vec<(String, Value)>),
}

impl Value {
    /// Build an empty `Map`.
    pub fn map() -> Self {
        Value::Map(Vec::new())
    }

    /// Look up a key in a `Map`; `None` for non-maps or missing keys.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Insert or overwrite a key in a `Map` (no-op on non-maps). Insertion order
    /// is preserved; an existing key is updated in place.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Value::Map(pairs) = self {
            if let Some(slot) = pairs.iter_mut().find(|(k, _)| k == key) {
                slot.1 = value;
            } else {
                pairs.push((key.to_string(), value));
            }
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

// Manual serde so Map serializes as a JSON object and List as a JSON array.
impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::Str(s) => serializer.serialize_str(s),
            Value::List(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Map(pairs) => {
                let mut map = serializer.serialize_map(Some(pairs.len()))?;
                for (k, v) in pairs {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON-like value")
            }

            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
                Ok(Value::Int(v))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
                if v <= i64::MAX as u64 {
                    Ok(Value::Int(v as i64))
                } else {
                    Ok(Value::Float(v as f64))
                }
            }
            fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
                Ok(Value::Float(v))
            }
            fn visit_str<E>(self, v: &str) -> Result<Value, E> {
                Ok(Value::Str(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> Result<Value, E> {
                Ok(Value::Str(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Value::List(items))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
                let mut pairs = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    pairs.push((k, v));
                }
                Ok(Value::Map(pairs))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

// Lua <-> Value conversions; round-trip is lossless for the supported set.
impl Value {
    /// Convert an arbitrary Lua value into a `Value`; `nil` becomes `Null`.
    pub fn from_lua(value: &mlua::Value) -> mlua::Result<Value> {
        Ok(match value {
            mlua::Value::Nil => Value::Null,
            mlua::Value::Boolean(b) => Value::Bool(*b),
            mlua::Value::Integer(i) => Value::Int(*i),
            mlua::Value::Number(n) => Value::Float(*n),
            mlua::Value::String(s) => Value::Str(s.to_str()?.to_string()),
            mlua::Value::Table(t) => Value::from_lua_table(t)?,
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
    pub fn from_lua_table(table: &mlua::Table) -> mlua::Result<Value> {
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
                items.push(Value::from_lua(&v)?);
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
                            message: Some(
                                "annotation payload keys must be string-like".to_string(),
                            ),
                        })
                    }
                };
                pairs.push((key, Value::from_lua(&v)?));
            }
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(Value::Map(pairs))
        }
    }

    /// Convert this `Value` into a Lua value (the inverse of [`Value::from_lua`]).
    pub fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        Ok(match self {
            Value::Null => mlua::Value::Nil,
            Value::Bool(b) => mlua::Value::Boolean(b),
            Value::Int(i) => mlua::Value::Integer(i),
            Value::Float(f) => mlua::Value::Number(f),
            Value::Str(s) => mlua::Value::String(lua.create_string(&s)?),
            Value::List(items) => {
                let t = lua.create_table()?;
                for (idx, item) in items.into_iter().enumerate() {
                    t.raw_set(idx + 1, item.into_lua(lua)?)?;
                }
                mlua::Value::Table(t)
            }
            Value::Map(pairs) => {
                let t = lua.create_table()?;
                for (k, v) in pairs {
                    t.raw_set(k, v.into_lua(lua)?)?;
                }
                mlua::Value::Table(t)
            }
        })
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod value_tests;
