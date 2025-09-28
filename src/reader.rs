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
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticCollector, DiagnosticLevel};
use crate::events::Header;
use crate::pointer::JsonPointer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    FullValidation,
    AppendSeek,
}

pub struct ArchiveReader {
    mode: ReadMode,
    filename: String,
}

#[derive(Debug)]
pub struct ReadResult {
    pub header: Header,
    pub final_state: Value,
    pub diagnostics: DiagnosticCollector,
    pub observation_count: usize,
}

impl ArchiveReader {
    pub fn new<P: AsRef<Path>>(path: P, mode: ReadMode) -> std::io::Result<Self> {
        let filename = path.as_ref().display().to_string();
        Ok(Self { mode, filename })
    }

    pub fn read<P: AsRef<Path>>(&self, path: P) -> std::io::Result<ReadResult> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut diagnostics = DiagnosticCollector::new();

        let mut lines_iter = reader.lines().enumerate();

        let (header_line_number, header_line) = match lines_iter.next() {
            Some((idx, Ok(line))) => (idx + 1, line),
            Some((idx, Err(e))) if e.kind() == std::io::ErrorKind::InvalidData => {
                let line_number = idx + 1;
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::InvalidUtf8,
                        format!("I found invalid UTF-8 bytes at line {}.", line_number)
                    )
                    .with_location(self.filename.clone(), line_number)
                    .with_advice(
                        "The JSON Archive format requires UTF-8 encoding. Make sure the file \
                         was saved with UTF-8 encoding, not Latin-1, Windows-1252, or another encoding."
                            .to_string()
                    )
                );
                return Ok(ReadResult {
                    header: Header::new(Value::Null, None),
                    final_state: Value::Null,
                    diagnostics,
                    observation_count: 0,
                });
            }
            Some((_, Err(e))) => return Err(e),
            None => {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::EmptyFile,
                        "I found an empty file, but I need at least a header line.".to_string(),
                    )
                    .with_location(self.filename.clone(), 1)
                    .with_advice(
                        "A valid JSON Archive file must start with a header object containing:\n\
                         - type: \"@peoplesgrocers/json-archive\"\n\
                         - version: 1\n\
                         - created: an ISO-8601 timestamp\n\
                         - initial: the initial state of the object"
                            .to_string(),
                    ),
                );
                return Ok(ReadResult {
                    header: Header::new(Value::Null, None),
                    final_state: Value::Null,
                    diagnostics,
                    observation_count: 0,
                });
            }
        };

        let header = match self.parse_header(&header_line, header_line_number, &mut diagnostics) {
            Some(h) => h,
            None => {
                return Ok(ReadResult {
                    header: Header::new(Value::Null, None),
                    final_state: Value::Null,
                    diagnostics,
                    observation_count: 0,
                });
            }
        };

        let mut state = header.initial.clone();
        let mut seen_observations: HashSet<String> = HashSet::new();
        let mut current_observation: Option<(String, usize, usize)> = None;
        let mut events_in_observation = 0;
        let mut observation_count = 0;

        // This manual dispatcher mirrors what serde would expand but stays explicit so we can
        // attach Elm-style diagnostics with precise spans and guidance for each failure case.
        for (idx, line_result) in lines_iter {
            let line_number = idx + 1;
            let line = match line_result {
                Ok(line) => line,
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidUtf8,
                            format!("I found invalid UTF-8 bytes at line {}.", line_number)
                        )
                        .with_location(self.filename.clone(), line_number)
                        .with_advice(
                            "The JSON Archive format requires UTF-8 encoding. Make sure the file \
                             was saved with UTF-8 encoding, not Latin-1, Windows-1252, or another encoding."
                                .to_string()
                        )
                    );
                    return Ok(ReadResult {
                        header: Header::new(Value::Null, None),
                        final_state: Value::Null,
                        diagnostics,
                        observation_count: 0,
                    });
                }
                Err(e) => return Err(e),
            };

            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }

            let event = match serde_json::from_str::<Value>(&line) {
                Ok(v) => v,
                Err(e) => {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidEventJson,
                            format!("I couldn't parse this line as JSON: {}", e),
                        )
                        .with_location(self.filename.clone(), line_number)
                        .with_snippet(format!("{} | {}", line_number, line))
                        .with_advice(
                            "Each line after the header must be either:\n\
                             - A comment starting with #\n\
                             - A valid JSON array representing an event\n\n\
                             Check for missing commas, quotes, or brackets."
                                .to_string(),
                        ),
                    );
                    continue;
                }
            };

            if let Some(arr) = event.as_array() {
                if arr.is_empty() {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldCount,
                            "I found an empty array, but events must have at least a type field."
                                .to_string(),
                        )
                        .with_location(self.filename.clone(), line_number)
                        .with_snippet(format!("{} | {}", line_number, line)),
                    );
                    continue;
                }

                let event_type = match arr[0].as_str() {
                    Some(t) => t,
                    None => {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::WrongFieldType,
                                "I expected the first element of an event to be a string event type.".to_string()
                            )
                            .with_location(self.filename.clone(), line_number)
                            .with_snippet(format!("{} | {}", line_number, line))
                            .with_advice(
                                "Events must look like [eventType, ...]. The eventType must be one of:\n\
                                 observe, add, change, remove, move, snapshot."
                                    .to_string()
                            )
                        );
                        continue;
                    }
                };

                match event_type {
                    "observe" => {
                        if let Some((_obs_id, obs_line, expected_count)) = &current_observation {
                            if events_in_observation != *expected_count {
                                diagnostics.add(
                                    Diagnostic::new(
                                        DiagnosticLevel::Warning,
                                        DiagnosticCode::ChangeCountMismatch,
                                        format!(
                                            "The observe event at line {} declared {} changes, but I found {}.",
                                            obs_line, expected_count, events_in_observation
                                        )
                                    )
                                    .with_location(self.filename.clone(), *obs_line)
                                    .with_advice(
                                        "Make sure the change_count in the observe event matches the number of \
                                         add/change/remove/move events that follow it."
                                            .to_string()
                                    )
                                );
                            }
                        }

                        if arr.len() != 4 {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::WrongFieldCount,
                                    format!("I expected an observe event to have 4 fields, but found {}.", arr.len())
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line))
                                .with_advice(
                                    "Observe events must be: [\"observe\", observationId, timestamp, changeCount]"
                                        .to_string()
                                )
                            );
                            continue;
                        }

                        let obs_id = arr[1].as_str().unwrap_or("").to_string();
                        let change_count = arr[3].as_u64().unwrap_or(0) as usize;

                        if seen_observations.contains(&obs_id) {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Warning,
                                    DiagnosticCode::DuplicateObservationId,
                                    format!("I found a duplicate observation ID: '{}'", obs_id),
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_advice(
                                    "Each observation ID should be unique within the archive. \
                                     Consider using UUIDs or timestamps to ensure uniqueness."
                                        .to_string(),
                                ),
                            );
                        }

                        seen_observations.insert(obs_id.clone());
                        current_observation = Some((obs_id, line_number, change_count));
                        events_in_observation = 0;
                        observation_count += 1;
                    }

                    "add" => {
                        events_in_observation += 1;
                        if arr.len() != 4 {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::WrongFieldCount,
                                    format!(
                                        "I expected an add event to have 4 fields, but found {}.",
                                        arr.len()
                                    ),
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line)),
                            );
                            continue;
                        }

                        let path = arr[1].as_str().unwrap_or("");
                        let value = arr[2].clone();
                        let obs_id = arr[3].as_str().unwrap_or("");

                        if self.mode == ReadMode::FullValidation
                            && !seen_observations.contains(obs_id)
                        {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::NonExistentObservationId,
                                    format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", obs_id)
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line))
                                .with_advice(
                                    "Each add/change/remove/move event must reference an observation ID from a preceding observe event."
                                        .to_string()
                                )
                            );
                            continue;
                        }

                        if let Err(_) =
                            self.apply_add(&mut state, path, value, line_number, &mut diagnostics)
                        {
                            continue;
                        }
                    }

                    "change" => {
                        events_in_observation += 1;
                        if arr.len() != 4 {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::WrongFieldCount,
                                    format!(
                                        "I expected a change event to have 4 fields, but found {}.",
                                        arr.len()
                                    ),
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line)),
                            );
                            continue;
                        }

                        let path = arr[1].as_str().unwrap_or("");
                        let new_value = arr[2].clone();
                        let obs_id = arr[3].as_str().unwrap_or("");

                        if self.mode == ReadMode::FullValidation
                            && !seen_observations.contains(obs_id)
                        {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::NonExistentObservationId,
                                    format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", obs_id)
                                )
                                .with_location(self.filename.clone(), line_number)
                            );
                            continue;
                        }

                        if let Err(_) = self.apply_change(
                            &mut state,
                            path,
                            new_value,
                            line_number,
                            &mut diagnostics,
                        ) {
                            continue;
                        }
                    }

                    "remove" => {
                        events_in_observation += 1;
                        if arr.len() != 3 {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::WrongFieldCount,
                                    format!(
                                        "I expected a remove event to have 3 fields, but found {}.",
                                        arr.len()
                                    ),
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line)),
                            );
                            continue;
                        }

                        let path = arr[1].as_str().unwrap_or("");
                        let obs_id = arr[2].as_str().unwrap_or("");

                        if self.mode == ReadMode::FullValidation
                            && !seen_observations.contains(obs_id)
                        {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::NonExistentObservationId,
                                    format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", obs_id)
                                )
                                .with_location(self.filename.clone(), line_number)
                            );
                            continue;
                        }

                        if let Err(_) =
                            self.apply_remove(&mut state, path, line_number, &mut diagnostics)
                        {
                            continue;
                        }
                    }

                    "move" => {
                        events_in_observation += 1;
                        if arr.len() != 4 {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::WrongFieldCount,
                                    format!(
                                        "I expected a move event to have 4 fields, but found {}.",
                                        arr.len()
                                    ),
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line)),
                            );
                            continue;
                        }

                        let path = arr[1].as_str().unwrap_or("");
                        let moves = arr[2].clone();
                        let obs_id = arr[3].as_str().unwrap_or("");

                        if self.mode == ReadMode::FullValidation
                            && !seen_observations.contains(obs_id)
                        {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::NonExistentObservationId,
                                    format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", obs_id)
                                )
                                .with_location(self.filename.clone(), line_number)
                            );
                            continue;
                        }

                        if let Err(_) =
                            self.apply_move(&mut state, path, moves, line_number, &mut diagnostics)
                        {
                            continue;
                        }
                    }

                    "snapshot" => {
                        if arr.len() != 4 {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::WrongFieldCount,
                                    format!("I expected a snapshot event to have 4 fields, but found {}.", arr.len())
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_snippet(format!("{} | {}", line_number, line))
                            );
                            continue;
                        }

                        let snapshot_state = arr[3].clone();

                        if self.mode == ReadMode::FullValidation && state != snapshot_state {
                            diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Warning,
                                    DiagnosticCode::SnapshotStateMismatch,
                                    "I found a snapshot whose state doesn't match the replayed state up to this point.".to_string()
                                )
                                .with_location(self.filename.clone(), line_number)
                                .with_advice(
                                    "This could indicate corruption or that events were applied incorrectly. \
                                     The snapshot state should exactly match the result of replaying all events \
                                     from the initial state."
                                        .to_string()
                                )
                            );
                        }

                        state = snapshot_state;
                    }

                    _ => {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Warning,
                                DiagnosticCode::UnknownEventType,
                                format!("I found an unknown event type: '{}'", event_type)
                            )
                            .with_location(self.filename.clone(), line_number)
                            .with_snippet(format!("{} | {}", line_number, line))
                            .with_advice(
                                "Valid event types are: observe, add, change, remove, move, snapshot. \
                                 This line will be skipped."
                                    .to_string()
                            )
                        );
                    }
                }
            } else {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldType,
                        "I expected an event to be a JSON array, but found a different type."
                            .to_string(),
                    )
                    .with_location(self.filename.clone(), line_number)
                    .with_snippet(format!("{} | {}", line_number, line)),
                );
            }
        }

        if let Some((_obs_id, obs_line, expected_count)) = &current_observation {
            if events_in_observation != *expected_count {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Warning,
                        DiagnosticCode::ChangeCountMismatch,
                        format!(
                            "The observe event at line {} declared {} changes, but I found {}.",
                            obs_line, expected_count, events_in_observation
                        ),
                    )
                    .with_location(self.filename.clone(), *obs_line),
                );
            }
        }

        Ok(ReadResult {
            header,
            final_state: state,
            diagnostics,
            observation_count,
        })
    }
    fn parse_header(
        &self,
        line: &str,
        line_number: usize,
        diagnostics: &mut DiagnosticCollector,
    ) -> Option<Header> {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::MissingHeader,
                        format!("I couldn't parse the header as JSON: {}", e),
                    )
                    .with_location(self.filename.clone(), line_number)
                    .with_snippet(format!("{} | {}", line_number, line))
                    .with_advice(
                        "The first line must be a JSON object containing the archive header.\n\
                         Required fields: type, version, created, initial"
                            .to_string(),
                    ),
                );
                return None;
            }
        };

        match serde_json::from_value::<Header>(value.clone()) {
            Ok(header) => {
                if header.version != 1 {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::UnsupportedVersion,
                            format!("I found version {}, but I only support version 1.", header.version)
                        )
                        .with_location(self.filename.clone(), line_number)
                        .with_advice(
                            "This archive was created with a newer or older version of the format. \
                             You may need to upgrade your tools or convert the archive."
                                .to_string()
                        )
                    );
                    return None;
                }

                Some(header)
            }
            Err(e) => {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::MissingHeaderField,
                        format!("I couldn't parse the header: {}", e),
                    )
                    .with_location(self.filename.clone(), line_number)
                    .with_snippet(format!("{} | {}", line_number, line))
                    .with_advice(
                        "The header must contain:\n\
                         - type: \"@peoplesgrocers/json-archive\"\n\
                         - version: 1\n\
                         - created: an ISO-8601 timestamp\n\
                         - initial: the initial state object"
                            .to_string(),
                    ),
                );
                None
            }
        }
    }

    fn apply_add(
        &self,
        state: &mut Value,
        path: &str,
        value: Value,
        line_number: usize,
        diagnostics: &mut DiagnosticCollector,
    ) -> Result<(), ()> {
        let pointer = match JsonPointer::new(path) {
            Ok(p) => p,
            Err(diag) => {
                diagnostics.add(
                    diag.with_location(self.filename.clone(), line_number)
                        .with_advice(
                            "JSON Pointer paths must start with '/' and use '/' to separate segments.\n\
                             Special characters: use ~0 for ~ and ~1 for /"
                                .to_string()
                        )
                );
                return Err(());
            }
        };

        if let Err(diag) = pointer.set(state, value) {
            diagnostics.add(
                diag.with_location(self.filename.clone(), line_number)
                    .with_advice(
                        "For add operations, the parent path must exist. \
                         For example, to add /a/b/c, the paths /a and /a/b must already exist."
                            .to_string(),
                    ),
            );
            return Err(());
        }

        Ok(())
    }

    fn apply_change(
        &self,
        state: &mut Value,
        path: &str,
        new_value: Value,
        line_number: usize,
        diagnostics: &mut DiagnosticCollector,
    ) -> Result<(), ()> {
        let pointer = match JsonPointer::new(path) {
            Ok(p) => p,
            Err(diag) => {
                diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                return Err(());
            }
        };

        if let Err(diag) = pointer.set(state, new_value) {
            diagnostics.add(diag.with_location(self.filename.clone(), line_number));
            return Err(());
        }

        Ok(())
    }

    fn apply_remove(
        &self,
        state: &mut Value,
        path: &str,
        line_number: usize,
        diagnostics: &mut DiagnosticCollector,
    ) -> Result<(), ()> {
        let pointer = match JsonPointer::new(path) {
            Ok(p) => p,
            Err(diag) => {
                diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                return Err(());
            }
        };

        if let Err(mut diag) = pointer.remove(state) {
            if self.mode == ReadMode::FullValidation {
                diag.level = DiagnosticLevel::Fatal;
            } else {
                diag.level = DiagnosticLevel::Warning;
            }

            diagnostics.add(diag.with_location(self.filename.clone(), line_number));

            if self.mode == ReadMode::FullValidation {
                return Err(());
            }
        }

        Ok(())
    }

    fn apply_move(
        &self,
        state: &mut Value,
        path: &str,
        moves_value: Value,
        line_number: usize,
        diagnostics: &mut DiagnosticCollector,
    ) -> Result<(), ()> {
        let pointer = match JsonPointer::new(path) {
            Ok(p) => p,
            Err(diag) => {
                diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                return Err(());
            }
        };

        let array = match pointer.get(state) {
            Ok(v) => {
                if !v.is_array() {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::MoveOnNonArray,
                            format!(
                                "I can't apply move operations to '{}' because it's not an array.",
                                path
                            ),
                        )
                        .with_location(self.filename.clone(), line_number)
                        .with_advice(
                            "Move operations can only reorder elements within an array. \
                             The path must point to an array value."
                                .to_string(),
                        ),
                    );
                    return Err(());
                }
                v.clone()
            }
            Err(diag) => {
                diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                return Err(());
            }
        };

        let mut arr = array.as_array().unwrap().clone();

        let moves = match moves_value.as_array() {
            Some(m) => m,
            None => {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::WrongFieldType,
                        "I expected the moves to be an array of [from, to] pairs.".to_string(),
                    )
                    .with_location(self.filename.clone(), line_number),
                );
                return Err(());
            }
        };

        for move_pair in moves {
            let pair = match move_pair.as_array() {
                Some(p) if p.len() == 2 => p,
                _ => {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::WrongFieldType,
                            "I expected each move to be a [from, to] pair.".to_string(),
                        )
                        .with_location(self.filename.clone(), line_number),
                    );
                    return Err(());
                }
            };

            let from_idx = match pair[0].as_u64() {
                Some(i) => i as usize,
                None => {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidMoveIndex,
                            "I expected the 'from' index to be a non-negative integer.".to_string(),
                        )
                        .with_location(self.filename.clone(), line_number),
                    );
                    return Err(());
                }
            };

            let to_idx = match pair[1].as_u64() {
                Some(i) => i as usize,
                None => {
                    diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidMoveIndex,
                            "I expected the 'to' index to be a non-negative integer.".to_string(),
                        )
                        .with_location(self.filename.clone(), line_number),
                    );
                    return Err(());
                }
            };

            if from_idx >= arr.len() {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::MoveIndexOutOfBounds,
                        format!(
                            "The 'from' index {} is out of bounds (array length is {}).",
                            from_idx,
                            arr.len()
                        ),
                    )
                    .with_location(self.filename.clone(), line_number),
                );
                return Err(());
            }

            if to_idx > arr.len() {
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::MoveIndexOutOfBounds,
                        format!(
                            "The 'to' index {} is out of bounds (array length is {}).",
                            to_idx,
                            arr.len()
                        ),
                    )
                    .with_location(self.filename.clone(), line_number),
                );
                return Err(());
            }

            let element = arr[from_idx].clone();
            arr.insert(to_idx, element);
            let remove_idx = if from_idx > to_idx {
                from_idx + 1
            } else {
                from_idx
            };
            arr.remove(remove_idx);
        }

        pointer.set(state, Value::Array(arr)).map_err(|diag| {
            diagnostics.add(diag.with_location(self.filename.clone(), line_number));
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_valid_archive() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;

        let header = Header::new(json!({"count": 0}), Some("test".to_string()));
        writeln!(temp_file, "{}", serde_json::to_string(&header)?)?;
        writeln!(
            temp_file,
            r#"["observe", "obs-1", "2025-01-01T00:00:00Z", 1]"#
        )?;
        writeln!(temp_file, r#"["change", "/count", 1, "obs-1"]"#)?;

        let reader = ArchiveReader::new(temp_file.path(), ReadMode::FullValidation)?;
        let result = reader.read(temp_file.path())?;

        assert_eq!(result.final_state, json!({"count": 1}));
        assert_eq!(result.observation_count, 1);
        assert!(!result.diagnostics.has_fatal());

        Ok(())
    }

    #[test]
    fn test_empty_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;

        let reader = ArchiveReader::new(temp_file.path(), ReadMode::FullValidation)?;
        let result = reader.read(temp_file.path())?;

        assert!(result.diagnostics.has_fatal());
        assert_eq!(result.diagnostics.len(), 1);

        Ok(())
    }

    #[test]
    fn test_non_existent_observation_id() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;

        let header = Header::new(json!({"count": 0}), None);
        writeln!(temp_file, "{}", serde_json::to_string(&header)?)?;
        writeln!(temp_file, r#"["change", "/count", 1, "obs-999"]"#)?;

        let reader = ArchiveReader::new(temp_file.path(), ReadMode::FullValidation)?;
        let result = reader.read(temp_file.path())?;

        assert!(result.diagnostics.has_fatal());

        Ok(())
    }

    #[test]
    fn test_append_mode_ignores_observation_id() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;

        let header = Header::new(json!({"count": 0}), None);
        writeln!(temp_file, "{}", serde_json::to_string(&header)?)?;
        writeln!(temp_file, r#"["change", "/count", 1, "obs-999"]"#)?;

        let reader = ArchiveReader::new(temp_file.path(), ReadMode::AppendSeek)?;
        let result = reader.read(temp_file.path())?;

        assert!(!result.diagnostics.has_fatal());
        assert_eq!(result.final_state, json!({"count": 1}));

        Ok(())
    }

    #[test]
    fn test_change_count_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;

        let header = Header::new(json!({"count": 0}), None);
        writeln!(temp_file, "{}", serde_json::to_string(&header)?)?;
        writeln!(
            temp_file,
            r#"["observe", "obs-1", "2025-01-01T00:00:00Z", 2]"#
        )?;
        writeln!(temp_file, r#"["change", "/count", 1, "obs-1"]"#)?;

        let reader = ArchiveReader::new(temp_file.path(), ReadMode::FullValidation)?;
        let result = reader.read(temp_file.path())?;

        let warnings: Vec<_> = result
            .diagnostics
            .diagnostics()
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .collect();

        assert_eq!(warnings.len(), 1);

        Ok(())
    }

    #[test]
    fn test_simple_change() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;

        let header = Header::new(json!({"count": 5}), None);
        writeln!(temp_file, "{}", serde_json::to_string(&header)?)?;
        writeln!(
            temp_file,
            r#"["observe", "obs-1", "2025-01-01T00:00:00Z", 1]"#
        )?;
        writeln!(temp_file, r#"["change", "/count", 1, "obs-1"]"#)?;

        let reader = ArchiveReader::new(temp_file.path(), ReadMode::FullValidation)?;
        let result = reader.read(temp_file.path())?;

        assert!(!result.diagnostics.has_fatal());
        assert_eq!(result.final_state, json!({"count": 1}));

        Ok(())
    }
}
