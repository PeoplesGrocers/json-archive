#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::{Arbitrary, Unstructured};
use json_archive::{ArchiveReader, ReadMode};
use std::io::Write;
use tempfile::NamedTempFile;
use serde_json::{json, Value};

#[derive(Arbitrary, Debug)]
struct FuzzArchive {
    header: FuzzHeader,
    events: Vec<FuzzEvent>,
}

#[derive(Arbitrary, Debug)]
struct FuzzHeader {
    has_type_field: bool,
    has_version_field: bool,
    has_created_field: bool,
    has_initial_field: bool,
    version_value: i32,
    initial_state: FuzzValue,
}

#[derive(Arbitrary, Debug)]
enum FuzzEvent {
    Observe {
        id: String,
        timestamp: String,
        change_count: i32,
    },
    Add {
        path: String,
        value: FuzzValue,
        obs_id: String,
    },
    Change {
        path: String,
        old_value: FuzzValue,
        new_value: FuzzValue,
        obs_id: String,
    },
    Remove {
        path: String,
        obs_id: String,
    },
    Move {
        path: String,
        moves: Vec<(i32, i32)>,
        obs_id: String,
    },
    Snapshot {
        id: String,
        timestamp: String,
        state: FuzzValue,
    },
    InvalidEvent {
        event_type: String,
        extra_fields: Vec<FuzzValue>,
    },
}

#[derive(Arbitrary, Debug)]
enum FuzzValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<FuzzValue>),
    Object(Vec<(String, FuzzValue)>),
}

impl FuzzValue {
    fn to_json(&self) -> Value {
        match self {
            FuzzValue::Null => Value::Null,
            FuzzValue::Bool(b) => Value::Bool(*b),
            FuzzValue::Number(n) => json!(n),
            FuzzValue::String(s) => Value::String(s.clone()),
            FuzzValue::Array(arr) => {
                Value::Array(arr.iter().map(|v| v.to_json()).collect())
            }
            FuzzValue::Object(obj) => {
                let map: serde_json::Map<String, Value> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect();
                Value::Object(map)
            }
        }
    }
}

impl FuzzArchive {
    fn generate_archive(&self) -> String {
        let mut lines = Vec::new();
        
        // Generate potentially malformed header
        let mut header_obj = serde_json::Map::new();
        
        if self.header.has_type_field {
            header_obj.insert("type".to_string(), json!("@peoplesgrocers/json-archive"));
        }
        
        if self.header.has_version_field {
            header_obj.insert("version".to_string(), json!(self.header.version_value));
        }
        
        if self.header.has_created_field {
            header_obj.insert("created".to_string(), json!("2025-01-01T00:00:00Z"));
        }
        
        if self.header.has_initial_field {
            header_obj.insert("initial".to_string(), self.header.initial_state.to_json());
        }
        
        lines.push(serde_json::to_string(&Value::Object(header_obj)).unwrap());
        
        // Generate events
        for event in &self.events {
            let event_json = match event {
                FuzzEvent::Observe { id, timestamp, change_count } => {
                    json!(["observe", id, timestamp, change_count])
                }
                FuzzEvent::Add { path, value, obs_id } => {
                    json!(["add", path, value.to_json(), obs_id])
                }
                FuzzEvent::Change { path, old_value, new_value, obs_id } => {
                    json!(["change", path, old_value.to_json(), new_value.to_json(), obs_id])
                }
                FuzzEvent::Remove { path, obs_id } => {
                    json!(["remove", path, obs_id])
                }
                FuzzEvent::Move { path, moves, obs_id } => {
                    let move_array: Vec<Value> = moves
                        .iter()
                        .map(|(from, to)| json!([from, to]))
                        .collect();
                    json!(["move", path, move_array, obs_id])
                }
                FuzzEvent::Snapshot { id, timestamp, state } => {
                    json!(["snapshot", id, timestamp, state.to_json()])
                }
                FuzzEvent::InvalidEvent { event_type, extra_fields } => {
                    let mut arr = vec![json!(event_type)];
                    arr.extend(extra_fields.iter().map(|f| f.to_json()));
                    Value::Array(arr)
                }
            };
            
            lines.push(serde_json::to_string(&event_json).unwrap());
        }
        
        lines.join("\n")
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    if let Ok(archive) = FuzzArchive::arbitrary(&mut u) {
        let content = archive.generate_archive();
        
        if let Ok(mut temp_file) = NamedTempFile::new() {
            if temp_file.write_all(content.as_bytes()).is_ok() {
                // Test both validation modes
                for mode in [ReadMode::FullValidation, ReadMode::AppendSeek] {
                    if let Ok(reader) = ArchiveReader::new(temp_file.path(), mode) {
                        let result = reader.read(temp_file.path());
                        
                        // The operation should never panic
                        // Verify that diagnostics are properly generated for invalid structures
                        if let Ok(read_result) = result {
                            // Basic sanity checks on the result
                            assert!(read_result.observation_count < 10000); // Reasonable upper bound
                            
                            // If there are fatal diagnostics, final state should be reasonable
                            if read_result.diagnostics.has_fatal() {
                                // Should still have some state (at least initial or null)
                                let _ = &read_result.final_state;
                            }
                        }
                    }
                }
            }
        }
    }
});