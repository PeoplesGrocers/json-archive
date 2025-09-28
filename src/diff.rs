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

use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::events::Event;

pub fn diff(old: &Value, new: &Value, base_path: &str, observation_id: &str) -> Vec<Event> {
    let mut result = Vec::<Event>::new();
    diff_recursive(old, new, base_path, observation_id, &mut result);
    result
}

fn diff_recursive(
    old: &Value,
    new: &Value,
    path: &str,
    observation_id: &str,
    result: &mut Vec<Event>,
) {
    match (old, new) {
        (Value::Object(old_obj), Value::Object(new_obj)) => {
            diff_objects(old_obj, new_obj, path, observation_id, result);
        }
        (Value::Array(old_arr), Value::Array(new_arr)) => {
            diff_arrays(old_arr, new_arr, path, observation_id, result);
        }
        _ => {
            if old != new {
                result.push(Event::Change {
                    path: path.to_string(),
                    new_value: new.clone(),
                    observation_id: observation_id.to_string(),
                });
            }
        }
    }
}

fn diff_objects(
    old: &serde_json::Map<String, Value>,
    new: &serde_json::Map<String, Value>,
    base_path: &str,
    observation_id: &str,
    result: &mut Vec<Event>,
) {
    let old_keys: HashSet<&String> = old.keys().collect();
    let new_keys: HashSet<&String> = new.keys().collect();

    // Removed keys
    for key in old_keys.difference(&new_keys) {
        let path = format_path(base_path, key);
        result.push(Event::Remove {
            path,
            observation_id: observation_id.to_string(),
        });
    }

    // Added keys
    for key in new_keys.difference(&old_keys) {
        let path = format_path(base_path, key);
        result.push(Event::Add {
            path,
            value: new[*key].clone(),
            observation_id: observation_id.to_string(),
        });
    }

    // Changed keys
    for key in old_keys.intersection(&new_keys) {
        let path = format_path(base_path, key);
        let old_value = &old[*key];
        let new_value = &new[*key];

        if old_value != new_value {
            diff_recursive(old_value, new_value, &path, observation_id, result);
        }
    }
}

fn diff_arrays(
    old: &[Value],
    new: &[Value],
    base_path: &str,
    observation_id: &str,
    result: &mut Vec<Event>,
) {
    // Simple implementation: we'll use a more sophisticated approach for move detection
    let mut old_items: HashMap<String, (usize, &Value)> = HashMap::new();
    let mut new_items: HashMap<String, (usize, &Value)> = HashMap::new();

    // Create content-based indices for move detection
    for (i, value) in old.iter().enumerate() {
        let key = value_hash(value);
        old_items.insert(key, (i, value));
    }

    for (i, value) in new.iter().enumerate() {
        let key = value_hash(value);
        new_items.insert(key, (i, value));
    }

    let old_keys: HashSet<&String> = old_items.keys().collect();
    let new_keys: HashSet<&String> = new_items.keys().collect();
    let common_keys: HashSet<&String> = old_keys.intersection(&new_keys).cloned().collect();

    // Track which items have been processed
    let mut processed_old: HashSet<usize> = HashSet::new();
    let mut processed_new: HashSet<usize> = HashSet::new();

    // Handle items that exist in both arrays (potential moves or unchanged)
    let mut moves: Vec<(usize, usize)> = Vec::new();

    for key in &common_keys {
        let (old_idx, old_val) = old_items[*key];
        let (new_idx, new_val) = new_items[*key];

        processed_old.insert(old_idx);
        processed_new.insert(new_idx);

        if old_val != new_val {
            // Value changed
            let path = format!("{}/{}", base_path, new_idx);
            result.push(Event::Change {
                path,
                new_value: new_val.clone(),
                observation_id: observation_id.to_string(),
            });
        } else if old_idx != new_idx {
            // Same value, different position - this is a move
            moves.push((old_idx, new_idx));
        }
    }

    // Generate move events if any
    if !moves.is_empty() {
        // Sort moves by original position to ensure consistent ordering
        moves.sort_by_key(|(old_idx, _)| *old_idx);
        result.push(Event::Move {
            path: base_path.to_string(),
            moves,
            observation_id: observation_id.to_string(),
        });
    }

    // Handle removed items (in old but not in new)
    let mut removed_indices: Vec<usize> = (0..old.len())
        .filter(|i| !processed_old.contains(i))
        .collect();

    // Remove from highest index to lowest to avoid index shifting issues
    removed_indices.sort_by(|a, b| b.cmp(a));

    for idx in removed_indices {
        let path = format!("{}/{}", base_path, idx);
        result.push(Event::Remove {
            path,
            observation_id: observation_id.to_string(),
        });
    }

    // Handle added items (in new but not in old)
    for i in 0..new.len() {
        if !processed_new.contains(&i) {
            let path = format!("{}/{}", base_path, i);
            result.push(Event::Add {
                path,
                value: new[i].clone(),
                observation_id: observation_id.to_string(),
            });
        }
    }
}

fn format_path(base: &str, segment: &str) -> String {
    let escaped_segment = segment.replace("~", "~0").replace("/", "~1");
    if base.is_empty() {
        format!("/{}", escaped_segment)
    } else {
        format!("{}/{}", base, escaped_segment)
    }
}

fn value_hash(value: &Value) -> String {
    // Simple content-based hash for move detection
    // In a real implementation, you might want a more sophisticated hash
    format!("{:?}", value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_object_add() {
        let old = json!({"a": 1});
        let new = json!({"a": 1, "b": 2});
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Add { path, value, .. } => {
                assert_eq!(path, "/b");
                assert_eq!(value, &json!(2));
            }
            _ => panic!("Expected Add event"),
        }
    }

    #[test]
    fn test_object_remove() {
        let old = json!({"a": 1, "b": 2});
        let new = json!({"a": 1});
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Remove { path, .. } => {
                assert_eq!(path, "/b");
            }
            _ => panic!("Expected Remove event"),
        }
    }

    #[test]
    fn test_object_change() {
        let old = json!({"a": 1});
        let new = json!({"a": 2});
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Change {
                path, new_value, ..
            } => {
                assert_eq!(path, "/a");
                assert_eq!(new_value, &json!(2));
            }
            _ => panic!("Expected Change event"),
        }
    }

    #[test]
    fn test_nested_object() {
        let old = json!({"user": {"name": "Alice", "age": 30}});
        let new = json!({"user": {"name": "Alice", "age": 31}});
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Change {
                path, new_value, ..
            } => {
                assert_eq!(path, "/user/age");
                assert_eq!(new_value, &json!(31));
            }
            _ => panic!("Expected Change event"),
        }
    }

    #[test]
    fn test_array_add() {
        let old = json!(["a", "b"]);
        let new = json!(["a", "b", "c"]);
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Add { path, value, .. } => {
                assert_eq!(path, "/2");
                assert_eq!(value, &json!("c"));
            }
            _ => panic!("Expected Add event"),
        }
    }

    #[test]
    fn test_array_remove() {
        let old = json!(["a", "b", "c"]);
        let new = json!(["a", "b"]);
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Remove { path, .. } => {
                assert_eq!(path, "/2");
            }
            _ => panic!("Expected Remove event"),
        }
    }

    #[test]
    fn test_array_move() {
        let old = json!(["a", "b", "c"]);
        let new = json!(["c", "a", "b"]);
        let result = diff(&old, &new, "", "obs-1");

        // Should generate move events
        assert!(!result.is_empty());

        // Check if we have a move event
        let has_move = result.iter().any(|e| matches!(e, Event::Move { .. }));
        assert!(has_move, "Expected at least one Move event");
    }

    #[test]
    fn test_escape_sequences_in_keys() {
        let old = json!({});
        let new = json!({"foo/bar": "value", "foo~bar": "value2"});
        let result = diff(&old, &new, "", "obs-1");

        assert_eq!(result.len(), 2);

        let paths: Vec<&String> = result
            .iter()
            .filter_map(|e| match e {
                Event::Add { path, .. } => Some(path),
                _ => None,
            })
            .collect();

        assert!(paths.contains(&&"/foo~1bar".to_string()));
        assert!(paths.contains(&&"/foo~0bar".to_string()));
    }
}
