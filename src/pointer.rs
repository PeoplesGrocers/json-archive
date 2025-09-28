// json-archive is a tool for tracking JSON file changes over time
// Copyright (C) 2025  Peoples Grocers LLC
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// To purchase a license under different terms contact admin@peoplesgrocers.com
// To request changes, report bugs, or give user feedback contact
// marxism@peoplesgrocers.com
//

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticLevel};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct JsonPointer {
    tokens: Vec<String>,
}

impl JsonPointer {
    pub fn new(path: &str) -> Result<Self, Diagnostic> {
        if path.is_empty() {
            return Ok(JsonPointer { tokens: vec![] });
        }

        if !path.starts_with('/') {
            return Err(Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::InvalidPointerSyntax,
                format!(
                    "I couldn't parse the path '{}': Path must start with '/'",
                    path
                ),
            ));
        }

        let tokens = path[1..]
            .split('/')
            .map(|token| token.replace("~1", "/").replace("~0", "~"))
            .collect();

        Ok(JsonPointer { tokens })
    }

    pub fn get<'a>(&self, value: &'a Value) -> Result<&'a Value, Diagnostic> {
        let mut current = value;

        for token in &self.tokens {
            match current {
                Value::Object(obj) => {
                    current = obj.get(token).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::PathNotFound,
                            format!("I couldn't find the key '{}'", token),
                        )
                    })?;
                }
                Value::Array(arr) => {
                    let index = token.parse::<usize>().map_err(|_| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidArrayIndex,
                            format!("I couldn't parse '{}' as an array index", token),
                        )
                    })?;
                    current = arr.get(index).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::PathNotFound,
                            format!(
                                "I couldn't find index {} (array length is {})",
                                index,
                                arr.len()
                            ),
                        )
                    })?;
                }
                _ => {
                    return Err(Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::TypeMismatch,
                        format!(
                            "I can't index into {} with '{}'",
                            current.type_name(),
                            token
                        ),
                    ));
                }
            }
        }

        Ok(current)
    }

    pub fn set(&self, value: &mut Value, new_value: Value) -> Result<(), Diagnostic> {
        if self.tokens.is_empty() {
            *value = new_value;
            return Ok(());
        }

        let mut current = value;
        let last_token = &self.tokens[self.tokens.len() - 1];

        for token in &self.tokens[..self.tokens.len() - 1] {
            match current {
                Value::Object(obj) => {
                    current = obj.get_mut(token).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::PathNotFound,
                            format!("I couldn't find the key '{}'", token),
                        )
                    })?;
                }
                Value::Array(arr) => {
                    let index = token.parse::<usize>().map_err(|_| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidArrayIndex,
                            format!("I couldn't parse '{}' as an array index", token),
                        )
                    })?;
                    let array_len = arr.len();
                    current = arr.get_mut(index).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::PathNotFound,
                            format!(
                                "I couldn't find index {} (array length is {})",
                                index, array_len
                            ),
                        )
                    })?;
                }
                _ => {
                    return Err(Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::TypeMismatch,
                        format!(
                            "I can't index into {} with '{}'",
                            current.type_name(),
                            token
                        ),
                    ));
                }
            }
        }

        match current {
            Value::Object(obj) => {
                obj.insert(last_token.clone(), new_value);
            }
            Value::Array(arr) => {
                let index = last_token.parse::<usize>().map_err(|_| {
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::InvalidArrayIndex,
                        format!("I couldn't parse '{}' as an array index", last_token),
                    )
                })?;

                if index == arr.len() {
                    arr.push(new_value);
                } else if index < arr.len() {
                    arr[index] = new_value;
                } else {
                    return Err(Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::PathNotFound,
                        format!(
                            "I couldn't set index {} (array length is {})",
                            index,
                            arr.len()
                        ),
                    ));
                }
            }
            _ => {
                return Err(Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::TypeMismatch,
                    format!(
                        "I can't set property '{}' on {}",
                        last_token,
                        current.type_name()
                    ),
                ));
            }
        }

        Ok(())
    }

    pub fn remove(&self, value: &mut Value) -> Result<Value, Diagnostic> {
        if self.tokens.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::InvalidPointerSyntax,
                "I can't remove the root value".to_string(),
            ));
        }

        let mut current = value;
        let last_token = &self.tokens[self.tokens.len() - 1];

        for token in &self.tokens[..self.tokens.len() - 1] {
            match current {
                Value::Object(obj) => {
                    current = obj.get_mut(token).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::PathNotFound,
                            format!("I couldn't find the key '{}'", token),
                        )
                    })?;
                }
                Value::Array(arr) => {
                    let index = token.parse::<usize>().map_err(|_| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidArrayIndex,
                            format!("I couldn't parse '{}' as an array index", token),
                        )
                    })?;
                    let array_len = arr.len();
                    current = arr.get_mut(index).ok_or_else(|| {
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::PathNotFound,
                            format!(
                                "I couldn't find index {} (array length is {})",
                                index, array_len
                            ),
                        )
                    })?;
                }
                _ => {
                    return Err(Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::TypeMismatch,
                        format!(
                            "I can't index into {} with '{}'",
                            current.type_name(),
                            token
                        ),
                    ));
                }
            }
        }

        match current {
            Value::Object(obj) => obj.remove(last_token).ok_or_else(|| {
                Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::PathNotFound,
                    format!("I couldn't find the key '{}' to remove", last_token),
                )
            }),
            Value::Array(arr) => {
                let index = last_token.parse::<usize>().map_err(|_| {
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::InvalidArrayIndex,
                        format!("I couldn't parse '{}' as an array index", last_token),
                    )
                })?;

                if index < arr.len() {
                    Ok(arr.remove(index))
                } else {
                    Err(Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::PathNotFound,
                        format!(
                            "I couldn't remove index {} (array length is {})",
                            index,
                            arr.len()
                        ),
                    ))
                }
            }
            _ => Err(Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::TypeMismatch,
                format!(
                    "I can't remove property '{}' from {}",
                    last_token,
                    current.type_name()
                ),
            )),
        }
    }

    pub fn to_string(&self) -> String {
        if self.tokens.is_empty() {
            return "".to_string();
        }

        let escaped_tokens: Vec<String> = self
            .tokens
            .iter()
            .map(|token| token.replace("~", "~0").replace("/", "~1"))
            .collect();

        format!("/{}", escaped_tokens.join("/"))
    }
}

trait ValueTypeExt {
    fn type_name(&self) -> &'static str;
}

impl ValueTypeExt for Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_pointer() {
        let pointer = JsonPointer::new("").unwrap();
        let value = json!({"foo": "bar"});
        assert_eq!(pointer.get(&value).unwrap(), &value);
    }

    #[test]
    fn test_simple_object_access() {
        let pointer = JsonPointer::new("/foo").unwrap();
        let value = json!({"foo": "bar"});
        assert_eq!(pointer.get(&value).unwrap(), &json!("bar"));
    }

    #[test]
    fn test_nested_object_access() {
        let pointer = JsonPointer::new("/foo/bar").unwrap();
        let value = json!({"foo": {"bar": "baz"}});
        assert_eq!(pointer.get(&value).unwrap(), &json!("baz"));
    }

    #[test]
    fn test_array_access() {
        let pointer = JsonPointer::new("/items/0").unwrap();
        let value = json!({"items": ["first", "second"]});
        assert_eq!(pointer.get(&value).unwrap(), &json!("first"));
    }

    #[test]
    fn test_escape_sequences() {
        let pointer = JsonPointer::new("/foo~1bar").unwrap();
        let value = json!({"foo/bar": "baz"});
        assert_eq!(pointer.get(&value).unwrap(), &json!("baz"));

        let pointer = JsonPointer::new("/foo~0bar").unwrap();
        let value = json!({"foo~bar": "baz"});
        assert_eq!(pointer.get(&value).unwrap(), &json!("baz"));
    }

    #[test]
    fn test_set_object() {
        let pointer = JsonPointer::new("/foo").unwrap();
        let mut value = json!({"foo": "bar"});
        pointer.set(&mut value, json!("new_value")).unwrap();
        assert_eq!(value, json!({"foo": "new_value"}));
    }

    #[test]
    fn test_set_array_append() {
        let pointer = JsonPointer::new("/items/2").unwrap();
        let mut value = json!({"items": ["first", "second"]});
        pointer.set(&mut value, json!("third")).unwrap();
        assert_eq!(value, json!({"items": ["first", "second", "third"]}));
    }

    #[test]
    fn test_remove_object() {
        let pointer = JsonPointer::new("/foo").unwrap();
        let mut value = json!({"foo": "bar", "baz": "qux"});
        let removed = pointer.remove(&mut value).unwrap();
        assert_eq!(removed, json!("bar"));
        assert_eq!(value, json!({"baz": "qux"}));
    }

    #[test]
    fn test_remove_array() {
        let pointer = JsonPointer::new("/items/0").unwrap();
        let mut value = json!({"items": ["first", "second", "third"]});
        let removed = pointer.remove(&mut value).unwrap();
        assert_eq!(removed, json!("first"));
        assert_eq!(value, json!({"items": ["second", "third"]}));
    }
}
