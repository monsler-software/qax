//! The value type that crosses the Rust <-> QML boundary. Deliberately small:
//! the four scalar kinds Qt's variant system maps cleanly onto Rust primitives.
//! Richer types (lists, nested models) are a planned extension point — see the
//! crate-level docs.

/// A value readable from / writable to a QML-exposed [`crate::Model`] field.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl Value {
    /// Returns the value as an integer, coercing from a float if needed.
    ///
    /// QML represents *all* numbers as doubles, so a field written from QML
    /// (`backend.count = 5`) arrives here as [`Value::Float`]. Coercing keeps
    /// `as_int()` working regardless of which side produced the number.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            _ => None,
        }
    }
    /// Returns the value as a float, coercing from an integer if needed.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(v) => Some(v),
            _ => None,
        }
    }
}

/// Anything that can be stored into a model field. Implemented for the scalar
/// primitives and `Value` itself, so `model.set("k", 1)` and
/// `model.set("k", "text")` both work without ceremony.
pub trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}
impl IntoValue for i64 {
    fn into_value(self) -> Value {
        Value::Int(self)
    }
}
impl IntoValue for i32 {
    fn into_value(self) -> Value {
        Value::Int(self as i64)
    }
}
impl IntoValue for f64 {
    fn into_value(self) -> Value {
        Value::Float(self)
    }
}
impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}
impl IntoValue for &str {
    fn into_value(self) -> Value {
        Value::Str(self.to_owned())
    }
}
impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::Str(self)
    }
}
