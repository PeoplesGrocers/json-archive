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
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticCollector, DiagnosticLevel};
use crate::event_deserialize::EventDeserializer;
use crate::events::{Event, Header};
use crate::pointer::JsonPointer;

#[cfg(feature = "compression")]
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
#[cfg(feature = "compression")]
use brotli::Decompressor;
#[cfg(feature = "compression")]
use zstd::stream::read::Decoder as ZstdDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    FullValidation,
    AppendSeek,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompressionFormat {
    Gzip,
    Deflate,
    Zlib,
    Brotli,
    Zstd,
    None,
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

pub struct EventIterator {
    reader: Box<dyn BufRead>,
    pub diagnostics: DiagnosticCollector,
    pub header: Header,
    filename: String,
    line_number: usize,
}

impl Iterator for EventIterator {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();

        loop {
            line.clear();
            self.line_number += 1;

            match self.reader.read_line(&mut line) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    let trimmed = line.trim();

                    // Skip comments and blank lines
                    if trimmed.starts_with('#') || trimmed.is_empty() {
                        continue;
                    }

                    // Try to parse as event
                    let event_deserializer = match serde_json::from_str::<EventDeserializer>(&line) {
                        Ok(d) => d,
                        Err(e) => {
                            self.diagnostics.add(
                                Diagnostic::new(
                                    DiagnosticLevel::Fatal,
                                    DiagnosticCode::InvalidEventJson,
                                    format!("I couldn't parse this line as JSON: {}", e),
                                )
                                .with_location(self.filename.clone(), self.line_number)
                                .with_snippet(format!("{} | {}", self.line_number, line.trim()))
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

                    // Add any diagnostics from deserialization
                    for diagnostic in event_deserializer.diagnostics {
                        self.diagnostics.add(
                            diagnostic
                                .with_location(self.filename.clone(), self.line_number)
                                .with_snippet(format!("{} | {}", self.line_number, line.trim()))
                        );
                    }

                    // Return event if we have one
                    if let Some(event) = event_deserializer.event {
                        return Some(event);
                    }

                    // If no event but had diagnostics, continue to next line
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    self.diagnostics.add(
                        Diagnostic::new(
                            DiagnosticLevel::Fatal,
                            DiagnosticCode::InvalidUtf8,
                            format!("I found invalid UTF-8 bytes at line {}.", self.line_number)
                        )
                        .with_location(self.filename.clone(), self.line_number)
                        .with_advice(
                            "The JSON Archive format requires UTF-8 encoding. Make sure the file \
                             was saved with UTF-8 encoding, not Latin-1, Windows-1252, or another encoding."
                                .to_string()
                        )
                    );
                    return None;
                }
                Err(_) => return None,
            }
        }
    }
}

fn detect_compression_format(path: &Path, bytes: &[u8]) -> CompressionFormat {
    if bytes.len() < 4 {
        return CompressionFormat::None;
    }

    // Gzip magic number: 0x1f 0x8b
    if bytes[0] == 0x1f && bytes[1] == 0x8b {
        return CompressionFormat::Gzip;
    }

    // Zlib magic number: 0x78 followed by 0x01, 0x5e, 0x9c, or 0xda
    if bytes[0] == 0x78 && (bytes[1] == 0x01 || bytes[1] == 0x5e || bytes[1] == 0x9c || bytes[1] == 0xda) {
        return CompressionFormat::Zlib;
    }

    // Zstd magic number: 0x28 0xb5 0x2f 0xfd
    if bytes.len() >= 4 && bytes[0] == 0x28 && bytes[1] == 0xb5 && bytes[2] == 0x2f && bytes[3] == 0xfd {
        return CompressionFormat::Zstd;
    }

    // Check file extension for brotli (no reliable magic number) and deflate
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy();
        if ext_str == "br" || path.to_string_lossy().contains(".br.") {
            return CompressionFormat::Brotli;
        }
        if ext_str == "deflate" {
            return CompressionFormat::Deflate;
        }
    }

    CompressionFormat::None
}

impl ArchiveReader {
    pub fn new<P: AsRef<Path>>(path: P, mode: ReadMode) -> std::io::Result<Self> {
        let filename = path.as_ref().display().to_string();
        Ok(Self { mode, filename })
    }

    pub fn events<P: AsRef<Path>>(&self, path: P) -> std::io::Result<(Value, EventIterator)> {
        let path = path.as_ref();
        let mut file = File::open(path)?;

        // Detect compression format
        let mut magic_bytes = [0u8; 4];
        let bytes_read = file.read(&mut magic_bytes)?;
        let compression_format = detect_compression_format(path, &magic_bytes[..bytes_read]);

        // Re-open file to reset position
        file = File::open(path)?;

        let mut diagnostics = DiagnosticCollector::new();

        // Check if compression is detected but not supported
        #[cfg(not(feature = "compression"))]
        if compression_format != CompressionFormat::None {
            let format_name = match compression_format {
                CompressionFormat::Gzip => "gzip",
                CompressionFormat::Deflate => "deflate",
                CompressionFormat::Zlib => "zlib",
                CompressionFormat::Brotli => "brotli",
                CompressionFormat::Zstd => "zstd",
                CompressionFormat::None => unreachable!(),
            };

            diagnostics.add(
                Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::UnsupportedVersion,
                    format!("I detected a {}-compressed archive, but this build doesn't support compression.", format_name)
                )
                .with_location(self.filename.clone(), 1)
                .with_advice(
                    "This binary was built without compression support to reduce binary size and dependencies.\n\
                     You have two options:\n\
                     1. Install the version with compression support: cargo install json-archive --features compression\n\
                     2. Manually decompress the file first, then use this tool on the uncompressed archive"
                        .to_string()
                )
            );

            // Return dummy values with fatal diagnostic
            let iterator = EventIterator {
                reader: Box::new(BufReader::new(std::io::empty())),
                diagnostics,
                header: Header::new(Value::Null, None),
                filename: self.filename.clone(),
                line_number: 1,
            };
            return Ok((Value::Null, iterator));
        }

        // Create appropriate reader based on compression format
        #[cfg(feature = "compression")]
        let reader: Box<dyn BufRead> = match compression_format {
            CompressionFormat::Gzip => Box::new(BufReader::new(GzDecoder::new(file))),
            CompressionFormat::Deflate => Box::new(BufReader::new(DeflateDecoder::new(file))),
            CompressionFormat::Zlib => Box::new(BufReader::new(ZlibDecoder::new(file))),
            CompressionFormat::Brotli => Box::new(BufReader::new(Decompressor::new(file, 4096))),
            CompressionFormat::Zstd => Box::new(BufReader::new(ZstdDecoder::new(file)?)),
            CompressionFormat::None => Box::new(BufReader::new(file)),
        };

        #[cfg(not(feature = "compression"))]
        let reader: Box<dyn BufRead> = Box::new(BufReader::new(file));

        let mut reader = reader;
        let mut header_line = String::new();

        let _bytes_read = match reader.read_line(&mut header_line) {
            Ok(0) => {
                // Empty file
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::EmptyFile,
                        "I found an empty file, but I need at least a header line.".to_string(),
                    )
                    .with_location(self.filename.clone(), 1)
                    .with_advice(
                        "See the file format specification for header structure."
                            .to_string(),
                    ),
                );
                let iterator = EventIterator {
                    reader,
                    diagnostics,
                    header: Header::new(Value::Null, None),
                    filename: self.filename.clone(),
                    line_number: 1,
                };
                return Ok((Value::Null, iterator));
            }
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                // UTF-8 error
                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::InvalidUtf8,
                        "I found invalid UTF-8 bytes at line 1.".to_string()
                    )
                    .with_location(self.filename.clone(), 1)
                    .with_advice(
                        "The JSON Archive format requires UTF-8 encoding. Make sure the file \
                         was saved with UTF-8 encoding, not Latin-1, Windows-1252, or another encoding."
                            .to_string()
                    )
                );
                let iterator = EventIterator {
                    reader,
                    diagnostics,
                    header: Header::new(Value::Null, None),
                    filename: self.filename.clone(),
                    line_number: 1,
                };
                return Ok((Value::Null, iterator));
            }
            Err(e) => return Err(e),
        };

        let header = match self.parse_header(&header_line, 1, &mut diagnostics) {
            Some(h) => h,
            None => {
                let iterator = EventIterator {
                    reader,
                    diagnostics,
                    header: Header::new(Value::Null, None),
                    filename: self.filename.clone(),
                    line_number: 1,
                };
                return Ok((Value::Null, iterator));
            }
        };

        let iterator = EventIterator {
            reader,
            diagnostics,
            header: header.clone(),
            filename: self.filename.clone(),
            line_number: 1,
        };

        Ok((header.initial, iterator))
    }

    pub fn read<P: AsRef<Path>>(&self, path: P) -> std::io::Result<ReadResult> {
        let (initial_value, mut event_iter) = self.events(&path)?;

        // Check for early fatal diagnostics (like compression not supported)
        if event_iter.diagnostics.has_fatal() {
            return Ok(ReadResult {
                header: Header::new(Value::Null, None),
                final_state: Value::Null,
                diagnostics: event_iter.diagnostics,
                observation_count: 0,
            });
        }

        let header = Header::new(initial_value.clone(), None);
        let mut state = initial_value;
        let mut seen_observations: HashSet<String> = HashSet::new();
        let mut current_observation: Option<(String, usize, usize)> = None;
        let mut events_in_observation = 0;
        let mut observation_count = 0;

        // Process events from iterator
        while let Some(event) = event_iter.next() {
            let line_number = event_iter.line_number;

            match event {
                Event::Observe { observation_id, timestamp: _, change_count } => {
                    if let Some((_obs_id, obs_line, expected_count)) = &current_observation {
                        if events_in_observation != *expected_count {
                            event_iter.diagnostics.add(
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
                        event_iter.diagnostics.add(
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
                        event_iter.diagnostics.add(
                            Diagnostic::new(
                                DiagnosticLevel::Fatal,
                                DiagnosticCode::NonExistentObservationId,
                                format!("I found a reference to observation '{}', but I haven't seen an observe event with that ID yet.", observation_id)
                            )
                            .with_location(self.filename.clone(), line_number)
                            .with_advice(
                                "Each add/change/remove/move event must reference an observation ID from a preceding observe event."
                                    .to_string()
                            )
                        );
                        continue;
                    }

                    if let Err(diag) = apply_add(&mut state, &path, value) {
                        event_iter.diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Change { path, new_value, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        event_iter.diagnostics.add(
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
                        event_iter.diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Remove { path, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        event_iter.diagnostics.add(
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
                        event_iter.diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Move { path, moves, observation_id } => {
                    events_in_observation += 1;

                    if self.mode == ReadMode::FullValidation
                        && !seen_observations.contains(&observation_id)
                    {
                        event_iter.diagnostics.add(
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
                        event_iter.diagnostics.add(diag.with_location(self.filename.clone(), line_number));
                        continue;
                    }
                }

                Event::Snapshot { observation_id: _, timestamp: _, object } => {
                    if self.mode == ReadMode::FullValidation && state != object {
                        event_iter.diagnostics.add(
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
                event_iter.diagnostics.add(
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
            diagnostics: event_iter.diagnostics,
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

pub fn apply_add(state: &mut Value, path: &str, value: Value) -> Result<(), Diagnostic> {
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

pub fn apply_change(state: &mut Value, path: &str, new_value: Value) -> Result<(), Diagnostic> {
    let pointer = JsonPointer::new(path)?;
    pointer.set(state, new_value)?;
    Ok(())
}

pub fn apply_remove(state: &mut Value, path: &str) -> Result<(), Diagnostic> {
    let pointer = JsonPointer::new(path)?;
    pointer.remove(state)?;
    Ok(())
}

pub fn apply_move(
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
