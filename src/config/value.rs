use toml::{Table, Value};

/// TOML value access, with the Python type names its messages used.
pub fn as_str(value: &Value) -> Option<&str> {
    value.as_str()
}

pub fn as_int(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(number) => Some(*number),
        Value::Float(number) => Some(*number as i64),
        _ => None,
    }
}

pub fn as_float(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(number) => Some(*number as f64),
        Value::Float(number) => Some(*number),
        _ => None,
    }
}

pub fn as_table(value: &Value) -> Option<&Table> {
    value.as_table()
}

/// A string, or the string form of any scalar — `config.command` used to `str()` values.
pub fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Integer(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::Boolean(flag) => flag.to_string(),
        Value::Datetime(stamp) => stamp.to_string(),
        other => other.to_string(),
    }
}

/// The Python name of the type, so a diagnosis reads the way it used to.
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "str",
        Value::Integer(_) => "int",
        Value::Float(_) => "float",
        Value::Boolean(_) => "bool",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "list",
        Value::Table(_) => "dict",
    }
}

/// A command written either as one string or as a list of them.
pub fn command_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(text) => Some(vec![text.clone()]),
        Value::Array(items) => Some(items.iter().map(render).collect()),
        _ => None,
    }
}

pub fn string_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => Some(items.iter().map(render).collect()),
        _ => None,
    }
}
