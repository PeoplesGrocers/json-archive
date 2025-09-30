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

//! Event deserialization with diagnostic collection.
//!
//! ## Why this exists
//!
//! The .json.archive format uses arrays for events because that's compact and easy to work
//! with in JavaScript: `["add", "/path", value, "obs-id"]`. The format is human-editable
//! since people might want to experiment with it or fix issues by hand.
//!
//! Two problems in Rust:
//!
//! 1. **Array-based format**: Serde derive expects named struct fields. Deserializing from
//!    positional arrays into structs requires custom Visitor implementation.
//!
//! 2. **Detailed error messages**: Goal is Elm-style diagnostics that show exactly what went
//!    wrong, what was expected, and how to fix it. Serde's Deserialize trait only allows
//!    returning string errors. To generate detailed diagnostics (with codes, severity levels,
//!    advice), we need to manually implement the Visitor and collect errors in a wrapper type
//!    instead of failing immediately. The wrapper gives us access to which field is being
//!    parsed so we can say "expected observation ID at position 3" instead of "parse error".
//!
//! ## Library search
//!
//! Spent 30 minutes looking for existing solutions. Checked:
//! - serde_path_to_error: Adds field path context but still returns string errors
//! - figment: Configuration library, but sounded like could be used only for diagnostics 
//! - config/serde_value: Similar issue
//! - json5: Relaxed JSON syntax, not diagnostic-focused
//! - miette: a diagnostic library for Rust. It includes a series of
//! traits/protocols that allow you to hook into its error reporting facilities,
//! and even write your own error reports. This is better than my home built
//! Diagnostic struct, but does not help me with deserialization.
//!
//! Found no library that handles both array deserialization and rich diagnostic collection.
//! This could probably be automated or turned into a library, but for a simple format it was
//! faster to implement by hand. Also serves as exploration of what diagnostic-driven parsing
//! costs in terms of code.
//!
//! ## What this does
//!
//! EventDeserializer wraps Event and collects diagnostics during parsing. It implements
//! Deserialize with a custom Visitor that validates each array position and populates the
//! diagnostics vec instead of returning errors. The calling code (reader.rs) attaches
//! location information (filename, line number) after deserialization.

use serde::de::{Deserialize, Deserializer, SeqAccess, Visitor};
use serde_json::Value;
use std::fmt;
use chrono::{DateTime, Utc};

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticLevel};
use crate::events::Event;

#[derive(Debug, Default)]
pub struct EventDeserializer {
    pub event: Option<Event>,
    pub diagnostics: Vec<Diagnostic>,
}

impl EventDeserializer {
    pub fn new() -> Self {
        Self::default()
    }

    fn add_diagnostic(&mut self, level: DiagnosticLevel, code: DiagnosticCode, message: String) {
        self.diagnostics.push(Diagnostic::new(level, code, message));
    }
}

impl<'de> Deserialize<'de> for EventDeserializer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(EventVisitor::new())
    }
}

struct EventVisitor {
    deserializer: EventDeserializer,
}

impl EventVisitor {
    fn new() -> Self {
        Self {
            deserializer: EventDeserializer::new(),
        }
    }
}

impl<'de> Visitor<'de> for EventVisitor {
    type Value = EventDeserializer;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an array representing an event")
    }

    fn visit_seq<A>(mut self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut elements: Vec<Value> = Vec::new();
        
        while let Some(elem) = seq.next_element::<Value>()? {
            elements.push(elem);
        }

        if elements.is_empty() {
            self.deserializer.add_diagnostic(
                DiagnosticLevel::Fatal,
                DiagnosticCode::WrongFieldCount,
                "I found an empty array, but events must have at least a string type field as first element.".to_string(),
            );
            return Ok(self.deserializer);
        }

        let event_type = match elements[0].as_str() {
            Some(t) => t,
            None => {
                self.deserializer.add_diagnostic(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::WrongFieldType,
                    "I expected the first element of an event to be a string event type.".to_string(),
                );
                return Ok(self.deserializer);
            }
        };

        match event_type {
            "observe" => {
                if elements.len() != 4 {
                    self.deserializer.add_diagnostic(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldCount,
                        format!("I expected an observe event to have 4 fields, but found {}.", elements.len()),
                    );
                    return Ok(self.deserializer);
                }

                let id = match elements[1].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the observation ID to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let timestamp = match elements[2].as_str() {
                    Some(s) => match s.parse::<DateTime<Utc>>() {
                        Ok(dt) => dt,
                        Err(_) => {
                            self.deserializer.add_diagnostic(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::WrongFieldType,
                                "I expected the timestamp to be a valid ISO-8601 datetime string.".to_string(),
                            );
                            return Ok(self.deserializer);
                        }
                    },
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the timestamp to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let change_count = match elements[3].as_u64() {
                    Some(n) => n as usize,
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the change count to be a non-negative integer.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                self.deserializer.event = Some(Event::Observe {
                    observation_id: id,
                    timestamp,
                    change_count,
                });
            }

            "add" => {
                if elements.len() != 4 {
                    self.deserializer.add_diagnostic(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldCount,
                        format!("I expected an add event to have 4 fields, but found {}.", elements.len()),
                    );
                    return Ok(self.deserializer);
                }

                let path = match elements[1].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the path to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let value = elements[2].clone();

                let observation_id = match elements[3].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the observation ID to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                self.deserializer.event = Some(Event::Add {
                    path,
                    value,
                    observation_id,
                });
            }

            "change" => {
                if elements.len() != 4 {
                    self.deserializer.add_diagnostic(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldCount,
                        format!("I expected a change event to have 4 fields, but found {}.", elements.len()),
                    );
                    return Ok(self.deserializer);
                }

                let path = match elements[1].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the path to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let new_value = elements[2].clone();

                let observation_id = match elements[3].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the observation ID to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                self.deserializer.event = Some(Event::Change {
                    path,
                    new_value,
                    observation_id,
                });
            }

            "remove" => {
                if elements.len() != 3 {
                    self.deserializer.add_diagnostic(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldCount,
                        format!("I expected a remove event to have 3 fields, but found {}.", elements.len()),
                    );
                    return Ok(self.deserializer);
                }

                let path = match elements[1].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the path to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let observation_id = match elements[2].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the observation ID to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                self.deserializer.event = Some(Event::Remove {
                    path,
                    observation_id,
                });
            }

            "move" => {
                if elements.len() != 4 {
                    self.deserializer.add_diagnostic(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldCount,
                        format!("I expected a move event to have 4 fields, but found {}.", elements.len()),
                    );
                    return Ok(self.deserializer);
                }

                let path = match elements[1].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the path to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let moves = match self.parse_moves(&elements[2]) {
                    Ok(moves) => moves,
                    Err(err_msg) => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            err_msg,
                        );
                        return Ok(self.deserializer);
                    }
                };

                let observation_id = match elements[3].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the observation ID to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                self.deserializer.event = Some(Event::Move {
                    path,
                    moves,
                    observation_id,
                });
            }

            "snapshot" => {
                if elements.len() != 4 {
                    self.deserializer.add_diagnostic(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldCount,
                        format!("I expected a snapshot event to have 4 fields, but found {}.", elements.len()),
                    );
                    return Ok(self.deserializer);
                }

                let observation_id = match elements[1].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the observation ID to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let timestamp = match elements[2].as_str() {
                    Some(s) => match s.parse::<DateTime<Utc>>() {
                        Ok(dt) => dt,
                        Err(_) => {
                            self.deserializer.add_diagnostic(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::WrongFieldType,
                                "I expected the timestamp to be a valid ISO-8601 datetime string.".to_string(),
                            );
                            return Ok(self.deserializer);
                        }
                    },
                    None => {
                        self.deserializer.add_diagnostic(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected the timestamp to be a string.".to_string(),
                        );
                        return Ok(self.deserializer);
                    }
                };

                let object = elements[3].clone();

                self.deserializer.event = Some(Event::Snapshot {
                    observation_id,
                    timestamp,
                    object,
                });
            }

            _ => {
                self.deserializer.add_diagnostic(
                    DiagnosticLevel::Warning,
                    DiagnosticCode::UnknownEventType,
                    format!("I found an unknown event type: '{}'", event_type),
                );
            }
        }

        Ok(self.deserializer)
    }
}

impl EventVisitor {
    fn parse_moves(&mut self, moves_value: &Value) -> Result<Vec<(usize, usize)>, String> {
        let moves_array = match moves_value.as_array() {
            Some(arr) => arr,
            None => {
                return Err("I expected the moves to be an array of [from, to] pairs.".to_string());
            }
        };

        let mut moves = Vec::new();
        for move_pair in moves_array {
            let pair = match move_pair.as_array() {
                Some(p) if p.len() == 2 => p,
                _ => {
                    return Err("I expected each move to be a [from, to] pair.".to_string());
                }
            };

            let from_idx = match pair[0].as_u64() {
                Some(i) => i as usize,
                None => {
                    return Err("I expected the 'from' index to be a non-negative integer.".to_string());
                }
            };

            let to_idx = match pair[1].as_u64() {
                Some(i) => i as usize,
                None => {
                    return Err("I expected the 'to' index to be a non-negative integer.".to_string());
                }
            };

            moves.push((from_idx, to_idx));
        }

        Ok(moves)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_observe_event() {
        let json = json!(["observe", "obs-1", "2025-01-01T00:00:00Z", 1]);
        let result: Result<EventDeserializer, _> = serde_json::from_value(json);
        
        assert!(result.is_ok());
        let deserializer = result.unwrap();
        assert!(deserializer.diagnostics.is_empty());
        assert!(matches!(
            deserializer.event,
            Some(Event::Observe { observation_id, timestamp: _, change_count })
            if observation_id == "obs-1" && change_count == 1
        ));
    }

    #[test]
    fn test_deserialize_add_event() {
        let json = json!(["add", "/count", 42, "obs-1"]);
        let result: Result<EventDeserializer, _> = serde_json::from_value(json);
        
        assert!(result.is_ok());
        let deserializer = result.unwrap();
        assert!(deserializer.diagnostics.is_empty());
        assert!(matches!(
            deserializer.event,
            Some(Event::Add { path, value, observation_id })
            if path == "/count" && value == json!(42) && observation_id == "obs-1"
        ));
    }

    #[test]
    fn test_deserialize_invalid_event_type() {
        let json = json!(["invalid", "some", "data"]);
        let result: Result<EventDeserializer, _> = serde_json::from_value(json);
        
        assert!(result.is_ok());
        let deserializer = result.unwrap();
        assert_eq!(deserializer.diagnostics.len(), 1);
        assert_eq!(deserializer.diagnostics[0].code, DiagnosticCode::UnknownEventType);
        assert!(deserializer.event.is_none());
    }

    #[test]
    fn test_deserialize_wrong_field_count() {
        let json = json!(["observe", "obs-1"]);
        let result: Result<EventDeserializer, _> = serde_json::from_value(json);
        
        assert!(result.is_ok());
        let deserializer = result.unwrap();
        assert_eq!(deserializer.diagnostics.len(), 1);
        assert_eq!(deserializer.diagnostics[0].code, DiagnosticCode::WrongFieldCount);
        assert!(deserializer.event.is_none());
    }

    #[test]
    fn test_deserialize_move_event() {
        let json = json!(["move", "/items", [[0, 2], [1, 0]], "obs-1"]);
        let result: Result<EventDeserializer, _> = serde_json::from_value(json);
        
        assert!(result.is_ok());
        let deserializer = result.unwrap();
        assert!(deserializer.diagnostics.is_empty());
        assert!(matches!(
            deserializer.event,
            Some(Event::Move { path, moves, observation_id })
            if path == "/items" && moves == vec![(0, 2), (1, 0)] && observation_id == "obs-1"
        ));
    }
}