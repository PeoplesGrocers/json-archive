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

use chrono::{DateTime, Utc};
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    /// Type identifier for magic file detection. We put this first to act as a "poor man's
    /// magic file number detection" when the archive file has an unexpected extension.
    /// This helps avoid issues like the Elm compiler that requires specific file extensions
    /// (e.g., .js) which doesn't play nice with build systems using temporary files with
    /// arbitrary suffixes. By putting this key first, we can detect archive files even
    /// when they have non-standard extensions like .tmp appended by build tools.
    #[serde(rename = "type")]
    pub file_type: String,
    pub version: u32,
    pub created: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub initial: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Header {
    pub fn new(initial: Value, source: Option<String>) -> Self {
        Self {
            file_type: "@peoplesgrocers/json-archive".to_string(),
            version: 1,
            created: Utc::now(),
            source,
            initial,
            metadata: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Observe {
        observation_id: String,
        timestamp: DateTime<Utc>,
        change_count: usize,
    },
    Add {
        path: String,
        value: Value,
        observation_id: String,
    },
    Change {
        path: String,
        new_value: Value,
        observation_id: String,
    },
    Remove {
        path: String,
        observation_id: String,
    },
    Move {
        path: String,
        moves: Vec<(usize, usize)>,
        observation_id: String,
    },
    Snapshot {
        observation_id: String,
        timestamp: DateTime<Utc>,
        object: Value,
    },
}


impl Serialize for Event {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;

        match self {
            Event::Observe {
                observation_id,
                timestamp,
                change_count,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("observe")?;
                seq.serialize_element(observation_id)?;
                seq.serialize_element(timestamp)?;
                seq.serialize_element(change_count)?;
                seq.end()
            }
            Event::Add {
                path,
                value,
                observation_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("add")?;
                seq.serialize_element(path)?;
                seq.serialize_element(value)?;
                seq.serialize_element(observation_id)?;
                seq.end()
            }
            Event::Change {
                path,
                new_value,
                observation_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("change")?;
                seq.serialize_element(path)?;
                seq.serialize_element(new_value)?;
                seq.serialize_element(observation_id)?;
                seq.end()
            }
            Event::Remove {
                path,
                observation_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element("remove")?;
                seq.serialize_element(path)?;
                seq.serialize_element(observation_id)?;
                seq.end()
            }
            Event::Move {
                path,
                moves,
                observation_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("move")?;
                seq.serialize_element(path)?;
                seq.serialize_element(moves)?;
                seq.serialize_element(observation_id)?;
                seq.end()
            }
            Event::Snapshot {
                observation_id,
                timestamp,
                object,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("snapshot")?;
                seq.serialize_element(observation_id)?;
                seq.serialize_element(timestamp)?;
                seq.serialize_element(object)?;
                seq.end()
            }
        }
    }
}

struct EventVisitor;

impl<'de> Visitor<'de> for EventVisitor {
    type Value = Event;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a JSON array representing an Event")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let event_type: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::missing_field("event type"))?;

        match event_type.as_str() {
            "observe" => {
                let observation_id: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("observation_id"))?;
                let timestamp: DateTime<Utc> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("timestamp"))?;
                let change_count: usize = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("change_count"))?;
                Ok(Event::Observe {
                    observation_id,
                    timestamp,
                    change_count,
                })
            }
            "add" => {
                let path: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("path"))?;
                let value: Value = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("value"))?;
                let observation_id: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("observation_id"))?;
                Ok(Event::Add {
                    path,
                    value,
                    observation_id,
                })
            }
            "change" => {
                let path: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("path"))?;
                let new_value: Value = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("new_value"))?;
                let observation_id: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("observation_id"))?;
                Ok(Event::Change {
                    path,
                    new_value,
                    observation_id,
                })
            }
            "remove" => {
                let path: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("path"))?;
                let observation_id: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("observation_id"))?;
                Ok(Event::Remove {
                    path,
                    observation_id,
                })
            }
            "move" => {
                let path: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("path"))?;
                let moves: Vec<(usize, usize)> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("moves"))?;
                let observation_id: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("observation_id"))?;
                Ok(Event::Move {
                    path,
                    moves,
                    observation_id,
                })
            }
            "snapshot" => {
                let observation_id: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("observation_id"))?;
                let timestamp: DateTime<Utc> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("timestamp"))?;
                let object: Value = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("object"))?;
                Ok(Event::Snapshot {
                    observation_id,
                    timestamp,
                    object,
                })
            }
            _ => Err(de::Error::unknown_variant(
                &event_type,
                &["observe", "add", "change", "remove", "move", "snapshot"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(EventVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub events: Vec<Event>,
}

impl Observation {
    pub fn new(id: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            id,
            timestamp,
            events: Vec::new(),
        }
    }

    pub fn add_event(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn to_events(self) -> Vec<Event> {
        let mut result = vec![Event::Observe {
            observation_id: self.id.clone(),
            timestamp: self.timestamp,
            change_count: self.events.len(),
        }];
        result.extend(self.events);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_header_serialization() {
        let header = Header::new(json!({"test": "value"}), Some("test-source".to_string()));
        let serialized = serde_json::to_string(&header).unwrap();
        let deserialized: Header = serde_json::from_str(&serialized).unwrap();

        assert_eq!(header.file_type, deserialized.file_type);
        assert_eq!(header.version, deserialized.version);
        assert_eq!(header.initial, deserialized.initial);
        assert_eq!(header.source, deserialized.source);
    }

    #[test]
    fn test_event_serialization() {
        let timestamp = Utc::now();

        // Test observe event
        let observe_event = Event::Observe {
            observation_id: "obs-1".to_string(),
            timestamp,
            change_count: 2,
        };
        let serialized = serde_json::to_string(&observe_event).unwrap();
        let expected_array = json!(["observe", "obs-1", timestamp, 2]);
        assert_eq!(
            serde_json::from_str::<Value>(&serialized).unwrap(),
            expected_array
        );

        // Test add event
        let add_event = Event::Add {
            path: "/test".to_string(),
            value: json!("value"),
            observation_id: "obs-1".to_string(),
        };
        let serialized = serde_json::to_string(&add_event).unwrap();
        let expected_array = json!(["add", "/test", "value", "obs-1"]);
        assert_eq!(
            serde_json::from_str::<Value>(&serialized).unwrap(),
            expected_array
        );

        // Test all event types for serialization/deserialization round-trip
        let events = vec![
            Event::Observe {
                observation_id: "obs-1".to_string(),
                timestamp,
                change_count: 2,
            },
            Event::Add {
                path: "/test".to_string(),
                value: json!("value"),
                observation_id: "obs-1".to_string(),
            },
            Event::Change {
                path: "/test".to_string(),
                new_value: json!("new"),
                observation_id: "obs-1".to_string(),
            },
            Event::Remove {
                path: "/test".to_string(),
                observation_id: "obs-1".to_string(),
            },
            Event::Move {
                path: "/items".to_string(),
                moves: vec![(0, 1)],
                observation_id: "obs-1".to_string(),
            },
            Event::Snapshot {
                observation_id: "snap-1".to_string(),
                timestamp,
                object: json!({"test": "state"}),
            },
        ];

        for event in events {
            let serialized = serde_json::to_string(&event).unwrap();

            // Verify it's serialized as an array
            let as_value: Value = serde_json::from_str(&serialized).unwrap();
            assert!(as_value.is_array(), "Event should serialize to JSON array");

            // Verify round-trip serialization/deserialization works
            let deserialized: Event = serde_json::from_str(&serialized).unwrap();
            let reserialized = serde_json::to_string(&deserialized).unwrap();
            assert_eq!(
                serialized, reserialized,
                "Round-trip serialization should be identical"
            );
        }
    }

    #[test]
    fn test_observation_to_events() {
        let mut obs = Observation::new("obs-1".to_string(), Utc::now());
        obs.add_event(Event::Add {
            path: "/test".to_string(),
            value: json!("value"),
            observation_id: "obs-1".to_string(),
        });
        obs.add_event(Event::Change {
            path: "/test".to_string(),
            new_value: json!("new"),
            observation_id: "obs-1".to_string(),
        });

        let events = obs.to_events();
        assert_eq!(events.len(), 3); // observe + 2 events

        match &events[0] {
            Event::Observe { change_count, .. } => assert_eq!(*change_count, 2),
            _ => panic!("First event should be observe"),
        }
    }
}
