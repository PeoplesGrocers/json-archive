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

use crate::flags;
use chrono::{DateTime, Utc};
use json_archive::reader::{ArchiveReader, ReadMode};
use json_archive::{Diagnostic, DiagnosticCode, DiagnosticLevel};
use serde_json::Value;
use std::path::Path;

#[derive(Debug)]
enum AccessMethod {
    Id(String),
    Index(usize),
    AsOf(DateTime<Utc>),
    RightBefore(DateTime<Utc>),
    After(DateTime<Utc>),
    Latest,
}

pub fn run(flags: &flags::State) -> Vec<Diagnostic> {
    if !flags.file.exists() {
        return vec![Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::PathNotFound,
            format!("I couldn't find the archive file: {}", flags.file.display()),
        )
        .with_advice(
            "Make sure the file path is correct and the file exists. \
                 Check for typos in the filename."
                .to_string(),
        )];
    }

    // Parse and validate flags - ensure only one access method is specified
    let access_method = match parse_access_method(flags) {
        Ok(method) => method,
        Err(diagnostic) => return vec![diagnostic],
    };

    // Read the archive using the existing reader
    let reader = match ArchiveReader::new(&flags.file, ReadMode::FullValidation) {
        Ok(reader) => reader,
        Err(e) => {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't open the archive file: {}", e),
            )];
        }
    };

    let result = match reader.read(&flags.file) {
        Ok(result) => result,
        Err(e) => {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't read the archive file: {}", e),
            )];
        }
    };

    // If there are fatal diagnostics, return them
    if result.diagnostics.has_fatal() {
        return result.diagnostics.into_diagnostics();
    }

    // For non-latest access methods, we need to collect observations and find the target
    let target_state = match access_method {
        AccessMethod::Latest => {
            // The reader already gives us the final state after all observations
            result.final_state
        }
        _ => {
            // We need to collect observations and replay to the target
            match find_and_replay_to_target(&flags.file, &access_method) {
                Ok(state) => state,
                Err(diagnostics) => return diagnostics,
            }
        }
    };

    // Output the JSON state
    match serde_json::to_string_pretty(&target_state) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::InvalidEventJson,
                format!("I couldn't serialize the state to JSON: {}", e),
            )];
        }
    }

    Vec::new()
}

fn parse_access_method(flags: &flags::State) -> Result<AccessMethod, Diagnostic> {
    let mut methods = Vec::new();

    if let Some(ref id) = flags.id {
        methods.push(AccessMethod::Id(id.clone()));
    }

    if let Some(index) = flags.index {
        methods.push(AccessMethod::Index(index));
    }

    if let Some(ref as_of_str) = flags.as_of {
        match as_of_str.parse::<DateTime<Utc>>() {
            Ok(dt) => methods.push(AccessMethod::AsOf(dt)),
            Err(_) => {
                return Err(Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::InvalidTimestamp,
                    format!("I couldn't parse the timestamp '{}'. Please use ISO-8601 format like '2025-01-15T10:05:00Z'", as_of_str)
                ));
            }
        }
    }

    if let Some(ref right_before_str) = flags.before {
        match right_before_str.parse::<DateTime<Utc>>() {
            Ok(dt) => methods.push(AccessMethod::RightBefore(dt)),
            Err(_) => {
                return Err(Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::InvalidTimestamp,
                    format!("I couldn't parse the timestamp '{}'. Please use ISO-8601 format like '2025-01-15T10:05:00Z'", right_before_str)
                ));
            }
        }
    }

    if let Some(ref after_str) = flags.after {
        match after_str.parse::<DateTime<Utc>>() {
            Ok(dt) => methods.push(AccessMethod::After(dt)),
            Err(_) => {
                return Err(Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::InvalidTimestamp,
                    format!("I couldn't parse the timestamp '{}'. Please use ISO-8601 format like '2025-01-15T10:05:00Z'", after_str)
                ));
            }
        }
    }

    if flags.latest.unwrap_or(false) {
        methods.push(AccessMethod::Latest);
    }

    match methods.len() {
        0 => Ok(AccessMethod::Latest), // Default to latest if no flags specified
        1 => Ok(methods.into_iter().next().unwrap()),
        _ => Err(Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::WrongFieldCount,
            "Please specify only one access method (--id, --index, --as-of, --right-before, --after, or --latest)".to_string()
        ).with_advice(
            "Examples:\n\
             json-archive state --id obs-123 file.archive\n\
             json-archive state --index 2 file.archive\n\
             json-archive state --as-of \"2025-01-15T10:05:00Z\" file.archive"
                .to_string()
        ))
    }
}

fn find_and_replay_to_target(
    file_path: &Path,
    access_method: &AccessMethod,
) -> Result<Value, Vec<Diagnostic>> {
    // We need to collect observations with full details to support all access methods
    let observations = collect_observations_with_events(file_path)?;

    if observations.is_empty() {
        return Err(vec![Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::EmptyFile,
            "No observations found in the archive".to_string(),
        )]);
    }

    // Find the target observation based on access method
    let target_observation = match access_method {
        AccessMethod::Id(id) => observations
            .iter()
            .find(|obs| obs.id == *id)
            .ok_or_else(|| {
                vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::NonExistentObservationId,
                    format!("I couldn't find an observation with ID '{}'", id),
                )
                .with_advice(
                    "Use 'json-archive info' to see available observation IDs".to_string(),
                )]
            })?,
        AccessMethod::Index(index) => {
            if *index >= observations.len() {
                return Err(vec![Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::ArrayIndexOutOfBounds,
                    format!(
                        "Index {} is out of bounds. The archive has {} observations (0-{})",
                        index,
                        observations.len(),
                        observations.len() - 1
                    ),
                )
                .with_advice(
                    "Use 'json-archive info' to see available observation indices".to_string(),
                )]);
            }
            &observations[*index]
        }
        AccessMethod::AsOf(timestamp) => {
            // Find most recent observation with timestamp <= given timestamp
            observations
                .iter()
                .filter(|obs| obs.timestamp <= *timestamp)
                .max_by_key(|obs| obs.timestamp)
                .ok_or_else(|| {
                    vec![Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::PathNotFound,
                        format!(
                            "No observations found as of {}",
                            timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                        ),
                    )
                    .with_advice(
                        "Try using --after to find the first observation after this time"
                            .to_string(),
                    )]
                })?
        }
        AccessMethod::RightBefore(timestamp) => {
            // Find most recent observation with timestamp < given timestamp (strictly before)
            observations
                .iter()
                .filter(|obs| obs.timestamp < *timestamp)
                .max_by_key(|obs| obs.timestamp)
                .ok_or_else(|| {
                    vec![Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::PathNotFound,
                        format!(
                            "No observations found before {}",
                            timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                        ),
                    )
                    .with_advice(
                        "Try using --as-of to include observations at exactly this time"
                            .to_string(),
                    )]
                })?
        }
        AccessMethod::After(timestamp) => {
            // Find earliest observation with timestamp > given timestamp
            observations.iter()
                .filter(|obs| obs.timestamp > *timestamp)
                .min_by_key(|obs| obs.timestamp)
                .ok_or_else(|| vec![
                    Diagnostic::new(
                        DiagnosticLevel::Fatal,
                        DiagnosticCode::PathNotFound,
                        format!("No observations found after {}", timestamp.format("%Y-%m-%d %H:%M:%S UTC"))
                    )
                    .with_advice("Try using --as-of to find the most recent observation before or at this time".to_string())
                ])?
        }
        AccessMethod::Latest => {
            // Find observation with latest timestamp
            observations.iter().max_by_key(|obs| obs.timestamp).unwrap() // Safe because we checked observations is not empty
        }
    };

    // Now replay events from initial state up to and including the target observation
    Ok(target_observation.final_state.clone())
}

#[derive(Debug)]
struct ObservationWithEvents {
    id: String,
    timestamp: DateTime<Utc>,
    final_state: Value,
}

fn collect_observations_with_events(
    file_path: &Path,
) -> Result<Vec<ObservationWithEvents>, Vec<Diagnostic>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = match File::open(file_path) {
        Ok(f) => f,
        Err(e) => {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't open the archive file: {}", e),
            )]);
        }
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut observations = Vec::new();

    // Parse header
    let header_line = match lines.next() {
        Some(Ok(line)) => line,
        _ => {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::EmptyFile,
                "Archive file is empty or unreadable".to_string(),
            )]);
        }
    };

    let header: Value = match serde_json::from_str(&header_line) {
        Ok(h) => h,
        Err(e) => {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::MissingHeader,
                format!("I couldn't parse the header: {}", e),
            )]);
        }
    };

    let created_str = header["created"].as_str().unwrap_or("");
    let created: DateTime<Utc> = match created_str.parse() {
        Ok(dt) => dt,
        Err(_) => Utc::now(),
    };

    let initial_state = header["initial"].clone();

    // Add initial state as observation 0
    observations.push(ObservationWithEvents {
        id: "initial".to_string(),
        timestamp: created,
        final_state: initial_state.clone(),
    });

    let mut current_state = initial_state;

    // Parse events and track state at each observation
    for line in lines {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let event: Value = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if let Some(arr) = event.as_array() {
            if arr.is_empty() {
                continue;
            }

            let event_type = arr[0].as_str().unwrap_or("");

            if event_type == "observe" && arr.len() >= 4 {
                let obs_id = arr[1].as_str().unwrap_or("").to_string();
                let timestamp_str = arr[2].as_str().unwrap_or("");

                let timestamp: DateTime<Utc> = match timestamp_str.parse() {
                    Ok(dt) => dt,
                    Err(_) => continue,
                };

                observations.push(ObservationWithEvents {
                    id: obs_id,
                    timestamp,
                    final_state: current_state.clone(), // Will be updated as events are applied
                });
            } else if event_type == "snapshot" && arr.len() >= 4 {
                // Handle snapshot events
                let obs_id = arr[1].as_str().unwrap_or("").to_string();
                let timestamp_str = arr[2].as_str().unwrap_or("");
                let snapshot_state = arr[3].clone();

                let timestamp: DateTime<Utc> = match timestamp_str.parse() {
                    Ok(dt) => dt,
                    Err(_) => continue,
                };

                current_state = snapshot_state.clone();
                observations.push(ObservationWithEvents {
                    id: obs_id,
                    timestamp,
                    final_state: snapshot_state,
                });
            } else {
                // Apply the event to current_state
                apply_event_to_state(&mut current_state, &arr);

                // Update the final state of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    last_obs.final_state = current_state.clone();
                }
            }
        }
    }

    Ok(observations)
}

fn apply_event_to_state(state: &mut Value, event: &[Value]) {
    if event.is_empty() {
        return;
    }

    let event_type = event[0].as_str().unwrap_or("");

    match event_type {
        "add" if event.len() >= 3 => {
            let path = event[1].as_str().unwrap_or("");
            let value = event[2].clone();
            if let Ok(pointer) = json_archive::pointer::JsonPointer::new(path) {
                let _ = pointer.set(state, value);
            }
        }
        "change" if event.len() >= 3 => {
            let path = event[1].as_str().unwrap_or("");
            let value = event[2].clone();
            if let Ok(pointer) = json_archive::pointer::JsonPointer::new(path) {
                let _ = pointer.set(state, value);
            }
        }
        "remove" if event.len() >= 2 => {
            let path = event[1].as_str().unwrap_or("");
            if let Ok(pointer) = json_archive::pointer::JsonPointer::new(path) {
                let _ = pointer.remove(state);
            }
        }
        "move" if event.len() >= 3 => {
            let path = event[1].as_str().unwrap_or("");
            let moves_value = event[2].clone();
            if let Ok(pointer) = json_archive::pointer::JsonPointer::new(path) {
                if let Ok(array_value) = pointer.get(state) {
                    if let Some(array) = array_value.as_array() {
                        let mut arr = array.clone();
                        if let Some(moves) = moves_value.as_array() {
                            for move_pair in moves {
                                if let Some(pair) = move_pair.as_array() {
                                    if pair.len() == 2 {
                                        if let (Some(from_idx), Some(to_idx)) = (
                                            pair[0].as_u64().map(|i| i as usize),
                                            pair[1].as_u64().map(|i| i as usize),
                                        ) {
                                            if from_idx < arr.len() && to_idx <= arr.len() {
                                                let element = arr[from_idx].clone();
                                                arr.insert(to_idx, element);
                                                let remove_idx = if from_idx > to_idx {
                                                    from_idx + 1
                                                } else {
                                                    from_idx
                                                };
                                                if remove_idx < arr.len() {
                                                    arr.remove(remove_idx);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let _ = pointer.set(state, Value::Array(arr));
                    }
                }
            }
        }
        _ => {}
    }
}
