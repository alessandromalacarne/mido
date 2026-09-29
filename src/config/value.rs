use toml::{Table, Value};

pub fn as_str(value: &Value) -> Option<&str> {
    value.as_str()
}

pub fn as_int(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(number) => Some(*number),
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

/// A list of strings; a command is one of these — the argv it runs as.
pub fn string_array(value: &Value) -> Option<Vec<String>> {
    let Value::Array(items) = value else {
        return None;
    };
    items
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_and_floats_read_as_numbers() {
        assert_eq!(as_int(&Value::Integer(300)), Some(300));
        assert_eq!(as_int(&Value::Float(300.0)), None);
        assert_eq!(as_float(&Value::Integer(20)), Some(20.0));
        assert_eq!(as_float(&Value::Float(12.5)), Some(12.5));
        assert_eq!(as_int(&Value::String("300".into())), None);
    }

    #[test]
    fn a_string_array_is_a_command() {
        assert_eq!(
            string_array(&Value::Array(vec![
                Value::String("cargo".into()),
                Value::String("test".into())
            ])),
            Some(vec!["cargo".to_string(), "test".to_string()])
        );
    }

    #[test]
    fn a_command_is_never_a_bare_string_or_a_mixed_list() {
        assert_eq!(string_array(&Value::String("cargo test".into())), None);
        assert_eq!(string_array(&Value::Integer(3)), None);
        assert_eq!(
            string_array(&Value::Array(vec![
                Value::String("cargo".into()),
                Value::Integer(2)
            ])),
            None
        );
    }
}
