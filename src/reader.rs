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
use crate::event_deserialize::EventDeserializer;
use crate::events::{Event, Header};
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

            let event_deserializer = match serde_json::from_str::<EventDeserializer>(&line) {
                Ok(d) => d,
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

            // Add any diagnostics from deserialization with location info
            for diagnostic in event_deserializer.diagnostics {
                diagnostics.add(
                    diagnostic
                        .with_location(self.filename.clone(), line_number)
                        .with_snippet(format!("{} | {}", line_number, line))
                );
            }

            // Continue processing to collect additional errors before failing.
            // Even though this function must now return an error, we continue to help
            // the user identify all issues in the file at once rather than one at a time.
            let event = match event_deserializer.event {
                Some(e) => e,
                None => {
                    assert!(diagnostics.has_fatal(), "Expected a fatal diagnostic when deserialization fails");
                    continue
                },
            };

            match event {
                Event::Observe { observation_id, timestamp: _, change_count } => {
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

                    if seen_observations.contains(&observation_id) {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Warning,
                                DiagnosticCode::DuplicateObservationId,
                                format!("I found a duplicate observation ID: '{}'", observation_id),
                            )
                            .with_location(self.filename.clone(), line_number)
                            .with_advice(
                                "Each observation ID should be unique within the archive. \
                                 Consider using UUIDs or timestamps to ensure uniqueness."
                                    .to_string(),
                            ),
                        );
                    }

                    seen_observations.insert(observation_id.clone());
                    current_observation = Some((observation_id, line_number, change_count));
                    events_in_observation = 0;
                    observation_count += 1;
                }

                Event::Add { path, value, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::NonExistentObservationId,
                                format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", observation_id)
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

                    if let Err(diag) = apply_add(&mut state, &path, value) {
                        diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Change { path, new_value, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::NonExistentObservationId,
                                format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", observation_id)
                            )
                            .with_location(self.filename.clone(), line_number)
                        );
                        continue;
                    }

                    if let Err(diag) = apply_change(&mut state, &path, new_value) {
                        diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Remove { path, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::NonExistentObservationId,
                                format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", observation_id)
                            )
                            .with_location(self.filename.clone(), line_number)
                        );
                        continue;
                    }

                    if let Err(diag) = apply_remove(&mut state, &path) {
                        diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Move { path, moves, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::NonExistentObservationId,
                                format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", observation_id)
                            )
                            .with_location(self.filename.clone(), line_number)
                        );
                        continue;
                    }

                    if let Err(diag) = apply_move(&mut state, &path, moves) {
                        diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Snapshot { observation_id: _, timestamp: _, object } => {
                    if self.mode == ReadMode::FullValidation && state != object {
                        diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
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

                    state = object;
                }
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

}

fn apply_add(state: &mut Value, path: &str, value: Value) -> Result<(), Diagnostic> {
    let pointer = JsonPointer::new(path).map_err(|diag| {
        diag.with_advice(
            "JSON Pointer paths must start with '/' and use '/' to separate segments.\n\
             Special characters: use ~0 for ~ and ~1 for /"
                .to_string()
        )
    })?;

    pointer.set(state, value).map_err(|diag| {
        diag.with_advice(
            "For add operations, the parent path must exist. \
             For example, to add /a/b/c, the paths /a and /a/b must already exist."
                .to_string()
        )
    })
}

fn apply_change(state: &mut Value, path: &str, new_value: Value) -> Result<(), Diagnostic> {
    let pointer = JsonPointer::new(path)?;
    pointer.set(state, new_value)?;
    Ok(())
}

fn apply_remove(state: &mut Value, path: &str) -> Result<(), Diagnostic> {
    let pointer = JsonPointer::new(path)?;
    pointer.remove(state)?;
    Ok(())
}

fn apply_move(
    state: &mut Value,
    path: &str,
    moves: Vec<(usize, usize)>,
) -> Result<(), Diagnostic> {
    let pointer = JsonPointer::new(path)?;

    let array = pointer.get(state)?;

    if !array.is_array() {
        return Err(
            Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::MoveOnNonArray,
                format!(
                    "I can't apply move operations to '{}' because it's not an array.",
                    path
                ),
            )
            .with_advice(
                "Move operations can only reorder elements within an array. \
                 The path must point to an array value."
                    .to_string(),
            ),
        );
    }

    let mut arr = array.as_array().unwrap().clone();

    for (from_idx, to_idx) in moves {
        if from_idx >= arr.len() {
            return Err(
                Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::MoveIndexOutOfBounds,
                    format!(
                        "The 'from' index {} is out of bounds (array length is {}).",
                        from_idx,
                        arr.len()
                    ),
                )
            );
        }

        if to_idx > arr.len() {
            return Err(
                Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::MoveIndexOutOfBounds,
                    format!(
                        "The 'to' index {} is out of bounds (array length is {}).",
                        to_idx,
                        arr.len()
                    ),
                )
            );
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

    pointer.set(state, Value::Array(arr))
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
