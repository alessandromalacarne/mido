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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_and_floats_read_as_numbers() {
        assert_eq!(as_int(&Value::Integer(300)), Some(300));
        assert_eq!(as_int(&Value::Float(300.0)), Some(300));
        assert_eq!(as_float(&Value::Integer(20)), Some(20.0));
        assert_eq!(as_int(&Value::String("300".into())), None);
    }

    #[test]
    fn scalar_rendering_covers_every_literal() {
        assert_eq!(render(&Value::String("cargo test".into())), "cargo test");
        assert_eq!(render(&Value::Integer(3)), "3");
        assert_eq!(render(&Value::Boolean(true)), "true");
    }

    #[test]
    fn commands_may_be_one_string_or_a_list() {
        assert_eq!(
            command_list(&Value::String("cargo test".into())),
            Some(vec!["cargo test".to_string()])
        );
        assert_eq!(
            command_list(&Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into())
            ])),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(command_list(&Value::Integer(3)), None);
    }

    #[test]
    fn python_type_names_survive() {
        assert_eq!(type_name(&Value::Integer(1)), "int");
        assert_eq!(type_name(&Value::Table(Table::new())), "dict");
        assert_eq!(type_name(&Value::Array(vec![])), "list");
    }
}
