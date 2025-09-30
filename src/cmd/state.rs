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
use json_archive::{apply_add, apply_change, apply_move, apply_remove, ArchiveReader, Diagnostic, DiagnosticCode, DiagnosticLevel, Event, ReadMode};
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

    // Find and replay to the target observation
    let target_state = match find_and_replay_to_target(&flags.file, &access_method) {
        Ok(state) => state,
        Err(diagnostics) => return diagnostics,
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
    let reader = match ArchiveReader::new(file_path, ReadMode::AppendSeek) {
        Ok(r) => r,
        Err(e) => {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't open the archive file: {}", e),
            )]);
        }
    };

    let (initial_state, mut event_iter) = match reader.events(file_path) {
        Ok(r) => r,
        Err(e) => {
            return Err(vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't read the archive file: {}", e),
            )]);
        }
    };

    // Check for fatal diagnostics from initial parsing
    if event_iter.diagnostics.has_fatal() {
        return Err(event_iter.diagnostics.diagnostics().to_vec());
    }

    // Collect observations while replaying events
    let mut observations = Vec::new();
    let mut current_state = initial_state.clone();
    let created = event_iter.header.created;

    // Add initial state as observation 0
    observations.push(ObservationWithEvents {
        id: "initial".to_string(),
        timestamp: created,
        final_state: initial_state,
    });

    // Process events and track state at each observation
    while let Some(event) = event_iter.next() {
        match event {
            Event::Observe { observation_id, timestamp, change_count: _ } => {
                observations.push(ObservationWithEvents {
                    id: observation_id,
                    timestamp,
                    final_state: current_state.clone(),
                });
            }
            Event::Add { path, value, .. } => {
                let _ = apply_add(&mut current_state, &path, value);

                // Update the final state of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.final_state = current_state.clone();
                    }
                }
            }
            Event::Change { path, new_value, .. } => {
                let _ = apply_change(&mut current_state, &path, new_value);

                // Update the final state of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.final_state = current_state.clone();
                    }
                }
            }
            Event::Remove { path, .. } => {
                let _ = apply_remove(&mut current_state, &path);

                // Update the final state of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.final_state = current_state.clone();
                    }
                }
            }
            Event::Move { path, moves, .. } => {
                let _ = apply_move(&mut current_state, &path, moves);

                // Update the final state of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.final_state = current_state.clone();
                    }
                }
            }
            Event::Snapshot { object, .. } => {
                current_state = object;

                // Update the final state of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.final_state = current_state.clone();
                    }
                }
            }
        }
    }

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

    Ok(target_observation.final_state.clone())
}

#[derive(Debug)]
struct ObservationWithEvents {
    id: String,
    timestamp: DateTime<Utc>,
    final_state: Value,
}
