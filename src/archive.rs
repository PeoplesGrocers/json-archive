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

use chrono::Utc;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticLevel};
use crate::diff;
use crate::events::{Event, Header, Observation};
use crate::reader::{ArchiveReader, ReadMode};

pub struct ArchiveWriter {
    writer: BufWriter<File>,
    observation_count: usize,
    snapshot_interval: Option<usize>,
    filename: String,
}

impl ArchiveWriter {
    pub fn new<P: AsRef<Path>>(
        path: P,
        snapshot_interval: Option<usize>,
    ) -> Result<Self, Vec<Diagnostic>> {
        let filename = path.as_ref().display().to_string();
        let file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                let diagnostic = Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::PathNotFound,
                    format!("I couldn't create the output file: {}", e)
                )
                .with_advice(
                    "Make sure you have write permission in this directory and that the path is valid."
                        .to_string()
                );
                return Err(vec![diagnostic]);
            }
        };
        let writer = BufWriter::new(file);

        Ok(Self {
            writer,
            observation_count: 0,
            snapshot_interval,
            filename,
        })
    }

    pub fn new_append<P: AsRef<Path>>(
        path: P,
        snapshot_interval: Option<usize>,
        current_observation_count: usize,
    ) -> Result<Self, Vec<Diagnostic>> {
        let filename = path.as_ref().display().to_string();
        let file = match OpenOptions::new().append(true).open(&path) {
            Ok(f) => f,
            Err(e) => {
                let diagnostic = Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::PathNotFound,
                    format!("I couldn't open the archive file for appending: {}", e)
                )
                .with_advice(
                    "Make sure the archive file exists and you have write permission."
                        .to_string()
                );
                return Err(vec![diagnostic]);
            }
        };
        let writer = BufWriter::new(file);

        Ok(Self {
            writer,
            observation_count: current_observation_count,
            snapshot_interval,
            filename,
        })
    }

    pub fn write_header(&mut self, header: &Header) -> Result<(), Vec<Diagnostic>> {
        let header_json = match serde_json::to_string(header) {
            Ok(json) => json,
            Err(e) => {
                return Err(vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::InvalidEventJson,
                    format!("I couldn't serialize the header to JSON: {}", e),
                )
                .with_location(self.filename.clone(), 1)]);
            }
        };

        if let Err(e) = writeln!(self.writer, "{}", header_json) {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't write to the output file: {}", e),
            )
            .with_location(self.filename.clone(), 1)]);
        }

        Ok(())
    }

    pub fn write_comment(&mut self, comment: &str) -> Result<(), Vec<Diagnostic>> {
        if let Err(e) = writeln!(self.writer, "# {}", comment) {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't write to the output file: {}", e),
            )]);
        }
        Ok(())
    }

    pub fn write_observation(&mut self, observation: Observation) -> Result<(), Vec<Diagnostic>> {
        let events = observation.to_events();

        for event in events {
            let event_json = match serde_json::to_string(&event) {
                Ok(json) => json,
                Err(e) => {
                    return Err(vec![Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::InvalidEventJson,
                        format!("I couldn't serialize an event to JSON: {}", e),
                    )]);
                }
            };

            if let Err(e) = writeln!(self.writer, "{}", event_json) {
                return Err(vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::PathNotFound,
                    format!("I couldn't write to the output file: {}", e),
                )]);
            }
        }

        self.observation_count += 1;
        Ok(())
    }

    pub fn write_snapshot(&mut self, object: &Value) -> Result<(), Vec<Diagnostic>> {
        let snapshot_id = format!("snapshot-{}", Uuid::new_v4());
        let snapshot = Event::Snapshot {
            observation_id: snapshot_id,
            timestamp: Utc::now(),
            object: object.clone(),
        };

        let event_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(e) => {
                return Err(vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::InvalidEventJson,
                    format!("I couldn't serialize the snapshot to JSON: {}", e),
                )]);
            }
        };

        if let Err(e) = writeln!(self.writer, "{}", event_json) {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't write to the output file: {}", e),
            )]);
        }

        Ok(())
    }

    pub fn should_write_snapshot(&self) -> bool {
        if let Some(interval) = self.snapshot_interval {
            self.observation_count > 0 && self.observation_count % interval == 0
        } else {
            false
        }
    }

    pub fn finish(mut self) -> Result<(), Vec<Diagnostic>> {
        if let Err(e) = self.writer.flush() {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't flush the output file: {}", e),
            )]);
        }
        Ok(())
    }
}

pub struct ArchiveBuilder {
    initial_state: Option<Value>,
    current_state: Value,
    source: Option<String>,
    snapshot_interval: Option<usize>,
}

impl ArchiveBuilder {
    pub fn new() -> Self {
        Self {
            initial_state: None,
            current_state: Value::Null,
            source: None,
            snapshot_interval: None,
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_snapshot_interval(mut self, interval: usize) -> Self {
        self.snapshot_interval = Some(interval);
        self
    }

    pub fn add_state(&mut self, state: Value) -> Option<Observation> {
        if self.initial_state.is_none() {
            self.initial_state = Some(state.clone());
            self.current_state = state;
            return None;
        }

        let observation_id = format!("obs-{}", Uuid::new_v4());
        let timestamp = Utc::now();

        let diff_result: Vec<Event> = diff::diff(&self.current_state, &state, "", &observation_id);
        self.current_state = state;

        let mut observation = Observation::new(observation_id, timestamp);
        for event in diff_result {
            observation.add_event(event);
        }

        Some(observation)
    }

    pub fn build<P: AsRef<Path>>(self, output_path: P) -> Result<(), Vec<Diagnostic>> {
        if self.initial_state.is_none() {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::MissingHeaderField,
                "I can't build an archive without any initial state.".to_string(),
            )]);
        }

        let header = Header::new(self.initial_state.unwrap(), self.source);

        let mut writer = ArchiveWriter::new(output_path, self.snapshot_interval)?;
        writer.write_header(&header)?;
        writer.finish()?;

        Ok(())
    }

    pub fn get_initial_state(&self) -> Option<&Value> {
        self.initial_state.as_ref()
    }
}

/// Generate default output filename from input filename
pub fn default_output_filename<P: AsRef<Path>>(input_path: P) -> PathBuf {
    let path = input_path.as_ref();
    let mut output = path.to_path_buf();

    // If it already ends with .json.archive, don't modify it
    if let Some(filename) = path.file_name() {
        if let Some(filename_str) = filename.to_str() {
            if filename_str.ends_with(".json.archive") {
                return output;
            }
        }
    }

    // Add .json.archive extension
    if let Some(extension) = path.extension() {
        if extension == "json" {
            // Replace .json with .json.archive
            output.set_extension("json.archive");
        } else {
            // Append .json.archive to whatever extension exists
            let new_extension = format!("{}.json.archive", extension.to_string_lossy());
            output.set_extension(new_extension);
        }
    } else {
        // No extension, just add .json.archive
        output.set_extension("json.archive");
    }

    output
}

pub fn create_archive_from_files<P: AsRef<Path>>(
    input_files: &[P],
    output_path: P,
    source: Option<String>,
    snapshot_interval: Option<usize>,
) -> Result<(), Vec<Diagnostic>> {
    let mut builder = ArchiveBuilder::new();
    if let Some(source) = source {
        builder = builder.with_source(source);
    }
    if let Some(interval) = snapshot_interval {
        builder = builder.with_snapshot_interval(interval);
    }

    let first_content = std::fs::read_to_string(&input_files[0]).map_err(|e| {
        vec![Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::PathNotFound,
            format!("I couldn't read the first input file: {}", e),
        )]
    })?;

    let first_state: Value = serde_json::from_str(&first_content).map_err(|e| {
        vec![Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::InvalidEventJson,
            format!("I couldn't parse the first input file as JSON: {}", e),
        )
        .with_advice("Make sure the file contains valid JSON.".to_string())]
    })?;

    let _ = builder.add_state(first_state.clone());

    let header = Header::new(first_state, builder.source.clone());
    let mut writer = ArchiveWriter::new(&output_path, builder.snapshot_interval)?;
    writer.write_header(&header)?;

    for file_path in input_files[1..].iter() {
        writer.write_comment(&format!("Processing file: {:?}", file_path.as_ref()))?;

        let content = std::fs::read_to_string(file_path).map_err(|e| {
            vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't read the input file: {}", e),
            )]
        })?;

        let state: Value = serde_json::from_str(&content).map_err(|e| {
            vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::InvalidEventJson,
                format!("I couldn't parse the input file as JSON: {}", e),
            )
            .with_advice("Make sure the file contains valid JSON.".to_string())]
        })?;

        if let Some(observation) = builder.add_state(state.clone()) {
            writer.write_observation(observation)?;

            if writer.should_write_snapshot() {
                writer.write_snapshot(&state)?;
            }
        }
    }

    writer.finish()?;
    Ok(())
}

pub fn append_to_archive<P: AsRef<Path>, Q: AsRef<Path>>(
    archive_path: P,
    new_files: &[Q],
    output_path: P,
    source: Option<String>,
    snapshot_interval: Option<usize>,
) -> Vec<Diagnostic> {
    // Read the existing archive to get the final state
    let reader = match ArchiveReader::new(&archive_path, ReadMode::AppendSeek) {
        Ok(r) => r,
        Err(e) => {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't open the archive for reading: {}", e),
            )];
        }
    };

    let read_result = match reader.read(&archive_path) {
        Ok(result) => result,
        Err(e) => {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't read the archive: {}", e),
            )];
        }
    };

    // Check for fatal diagnostics in the archive
    if read_result.diagnostics.has_fatal() {
        let mut diagnostics = vec![Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::InvalidEventJson,
            "The existing archive contains fatal errors. Cannot append to a corrupt archive.".to_string(),
        )];
        diagnostics.extend(read_result.diagnostics.into_diagnostics());
        return diagnostics;
    }

    // If output path is different from archive path, copy the archive first
    if archive_path.as_ref() != output_path.as_ref() {
        if let Err(e) = std::fs::copy(&archive_path, &output_path) {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't copy the archive to the output location: {}", e),
            )];
        }
    }

    // Create an append writer
    let mut writer = match ArchiveWriter::new_append(&output_path, snapshot_interval, read_result.observation_count) {
        Ok(w) => w,
        Err(diagnostics) => return diagnostics,
    };

    // Create a builder to track state changes
    let mut builder = ArchiveBuilder::new();
    if let Some(source) = source {
        builder = builder.with_source(source);
    }
    if let Some(interval) = snapshot_interval {
        builder = builder.with_snapshot_interval(interval);
    }

    // Initialize builder with the final state from the archive
    let current_state = read_result.final_state;
    builder.current_state = current_state.clone();
    builder.initial_state = Some(current_state.clone());

    // Process each new file
    for file_path in new_files.iter() {
        if let Err(diagnostics) = writer.write_comment(&format!("Processing file: {:?}", file_path.as_ref())) {
            return diagnostics;
        }

        let content = match std::fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(e) => {
                return vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::PathNotFound,
                    format!("I couldn't read the input file: {}", e),
                )];
            }
        };

        let state: Value = match serde_json::from_str(&content) {
            Ok(state) => state,
            Err(e) => {
                return vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::InvalidEventJson,
                    format!("I couldn't parse the input file as JSON: {}", e),
                )
                .with_advice("Make sure the file contains valid JSON.".to_string())];
            }
        };

        if let Some(observation) = builder.add_state(state.clone()) {
            if let Err(diagnostics) = writer.write_observation(observation) {
                return diagnostics;
            }

            if writer.should_write_snapshot() {
                if let Err(diagnostics) = writer.write_snapshot(&state) {
                    return diagnostics;
                }
            }
        }
    }

    // Finish writing
    match writer.finish() {
        Ok(()) => Vec::new(),
        Err(diagnostics) => diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_archive_writer_header() -> Result<(), Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        let header = Header::new(json!({"test": "value"}), Some("test-source".to_string()));

        {
            let mut writer = ArchiveWriter::new(temp_file.path(), None)
                .map_err(|_| "Failed to create writer")?;
            writer
                .write_header(&header)
                .map_err(|_| "Failed to write header")?;
            writer.finish().map_err(|_| "Failed to finish")?;
        }

        let content = std::fs::read_to_string(temp_file.path())?;
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let parsed_header: Header = serde_json::from_str(lines[0])?;
        assert_eq!(parsed_header.file_type, "@peoplesgrocers/json-archive");
        assert_eq!(parsed_header.version, 1);
        assert_eq!(parsed_header.initial, json!({"test": "value"}));

        Ok(())
    }

    #[test]
    fn test_archive_builder() -> Result<(), Box<dyn std::error::Error>> {
        let mut builder = ArchiveBuilder::new();

        // First state becomes initial
        let result = builder.add_state(json!({"count": 0}));
        assert!(result.is_none());

        // Second state generates observation
        let observation = builder
            .add_state(json!({"count": 1}))
            .expect("Should generate observation");
        assert!(!observation.events.is_empty());

        Ok(())
    }

    #[test]
    fn test_create_archive_from_files() -> Result<(), Box<dyn std::error::Error>> {
        // Create temporary input files
        let mut file1 = NamedTempFile::new()?;
        let mut file2 = NamedTempFile::new()?;
        let output_file = NamedTempFile::new()?;

        writeln!(file1, r#"{{"count": 0, "name": "test"}}"#)?;
        writeln!(file2, r#"{{"count": 1, "name": "test"}}"#)?;

        let input_files = vec![file1.path(), file2.path()];

        create_archive_from_files(
            &input_files,
            output_file.path(),
            Some("test-source".to_string()),
            None,
        )
        .map_err(|_| "Failed to create archive")?;

        let content = std::fs::read_to_string(output_file.path())?;
        let lines: Vec<&str> = content.lines().collect();

        assert!(lines.len() >= 2); // At least header + comment + observe + change events

        // First line should be header
        let header: Header = serde_json::from_str(lines[0])?;
        assert_eq!(header.file_type, "@peoplesgrocers/json-archive");
        assert_eq!(header.version, 1);
        assert_eq!(header.initial, json!({"count": 0, "name": "test"}));

        Ok(())
    }

    #[test]
    fn test_snapshot_interval() -> Result<(), Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        let mut writer =
            ArchiveWriter::new(temp_file.path(), Some(2)).map_err(|_| "Failed to create writer")?;

        assert!(!writer.should_write_snapshot()); // No observations yet

        let obs1 = Observation::new("obs-1".to_string(), Utc::now());
        writer
            .write_observation(obs1)
            .map_err(|_| "Failed to write observation")?;
        assert!(!writer.should_write_snapshot()); // 1 observation, interval is 2

        let obs2 = Observation::new("obs-2".to_string(), Utc::now());
        writer
            .write_observation(obs2)
            .map_err(|_| "Failed to write observation")?;
        assert!(writer.should_write_snapshot()); // 2 observations, should snapshot

        Ok(())
    }

    #[test]
    fn test_default_output_filename() {
        assert_eq!(
            default_output_filename("test.json"),
            PathBuf::from("test.json.archive")
        );

        assert_eq!(
            default_output_filename("test.txt"),
            PathBuf::from("test.txt.json.archive")
        );

        assert_eq!(
            default_output_filename("test"),
            PathBuf::from("test.json.archive")
        );

        assert_eq!(
            default_output_filename("test.json.archive"),
            PathBuf::from("test.json.archive")
        );
    }
}
