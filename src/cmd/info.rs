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
use json_archive::{Diagnostic, DiagnosticCode, DiagnosticLevel, Event};
use serde::Serialize;
use std::path::Path;

#[derive(Debug)]
struct ObservationInfo {
    id: String,
    timestamp: DateTime<Utc>,
    created: DateTime<Utc>, // For initial state, this is the archive creation time
    change_count: usize,
    json_size: usize,
}

#[derive(Serialize)]
struct JsonObservation {
    index: usize,
    id: String,
    timestamp: String,
    changes: usize,
    json_size: usize,
}

#[derive(Serialize)]
struct JsonInfoOutput {
    archive: String,
    created: String,
    file_size: u64,
    snapshot_count: usize,
    observations: Vec<JsonObservation>,
    total_json_size: u64,
    efficiency_percent: f64,
}

pub fn run(flags: &flags::Info) -> Vec<Diagnostic> {
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

    let (observations, snapshot_count) = match collect_observations(&flags.file) {
        Ok((obs, count)) => (obs, count),
        Err(diagnostics) => return diagnostics,
    };

    let file_size = match std::fs::metadata(&flags.file) {
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    };

    // Calculate total JSON size (sum of all observations + newline separators)
    let total_json_size: u64 = observations.iter().map(|obs| obs.json_size as u64).sum::<u64>()
        + (observations.len() as u64).saturating_sub(1); // Add newlines between observations

    let efficiency_percent = if total_json_size > 0 {
        (file_size as f64 / total_json_size as f64) * 100.0
    } else {
        0.0
    };

    // Check output format
    let is_json_output = flags.output.as_ref().map(|s| s == "json").unwrap_or(false);

    if is_json_output {
        // JSON output mode
        if observations.is_empty() {
            let empty_output = JsonInfoOutput {
                archive: flags.file.display().to_string(),
                created: "".to_string(),
                file_size,
                snapshot_count,
                observations: Vec::new(),
                total_json_size: 0,
                efficiency_percent: 0.0,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&empty_output).unwrap_or_default()
            );
            return Vec::new();
        }

        let json_observations: Vec<JsonObservation> = observations
            .iter()
            .enumerate()
            .map(|(index, obs)| JsonObservation {
                index,
                id: if index == 0 {
                    "initial".to_string()
                } else {
                    obs.id.clone()
                },
                timestamp: obs.timestamp.to_rfc3339(),
                changes: obs.change_count,
                json_size: obs.json_size,
            })
            .collect();

        let json_output = JsonInfoOutput {
            archive: flags.file.display().to_string(),
            created: observations[0].created.to_rfc3339(),
            file_size,
            snapshot_count,
            observations: json_observations,
            total_json_size,
            efficiency_percent,
        };

        println!(
            "{}",
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        );
    } else {
        // Human-readable output mode
        println!("Archive: {}", flags.file.display());

        if observations.is_empty() {
            println!("No observations found");
            return Vec::new();
        }

        let first_timestamp = &observations[0].created;
        let last_timestamp = if observations.len() > 1 {
            &observations.last().unwrap().timestamp
        } else {
            first_timestamp
        };

        println!("Created: {}", format_timestamp(first_timestamp));
        println!();

        if observations.len() == 1 {
            println!("1 observation on {}", format_timestamp(first_timestamp));
        } else {
            println!(
                "{} observations from {} to {}",
                observations.len(),
                format_timestamp(first_timestamp),
                format_timestamp(last_timestamp)
            );
        }
        println!();

        // Table header
        println!("  #  Observation ID                    Date & Time                  Changes  JSON Size");
        println!("────────────────────────────────────────────────────────────────────────────────────────");

        for (index, obs) in observations.iter().enumerate() {
            let id_display = if index == 0 {
                "(initial)".to_string()
            } else {
                truncate_id(&obs.id)
            };

            let changes_display = if index == 0 {
                "-".to_string()
            } else {
                obs.change_count.to_string()
            };

            println!(
                "  {:2}  {:32}  {:25}  {:7}  {:9}",
                index,
                id_display,
                format_timestamp(&obs.timestamp),
                changes_display,
                format_size(obs.json_size as u64)
            );
        }

        println!();
        let snapshot_text = if snapshot_count == 0 {
            "0 snapshots".to_string()
        } else {
            format!("{} snapshots", snapshot_count)
        };

        let comparison = if efficiency_percent < 100.0 {
            format!("{:.1}% smaller", 100.0 - efficiency_percent)
        } else {
            format!("{:.1}% larger", efficiency_percent - 100.0)
        };

        println!(
            "Archive size: {} ({}, {} than JSON Lines)",
            format_size(file_size),
            snapshot_text,
            comparison
        );
        println!(
            "Data size: {}",
            format_size(total_json_size)
        );

        // Add usage instructions
        println!();
        println!("To get the JSON value at a specific observation:");
        println!("  json-archive state --index <#> {}", flags.file.display());
        println!(
            "  json-archive state --id <observation-id> {}",
            flags.file.display()
        );
        println!();
        println!("Examples:");
        println!(
            "  json-archive state --index 0 {}    # Get initial state",
            flags.file.display()
        );
        println!(
            "  json-archive state --index 2 {}    # Get state after observation 2",
            flags.file.display()
        );
    }

    Vec::new()
}

fn collect_observations(file_path: &Path) -> Result<(Vec<ObservationInfo>, usize), Vec<Diagnostic>> {
    let reader = match json_archive::ArchiveReader::new(file_path, json_archive::ReadMode::AppendSeek) {
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

    let mut observations = Vec::new();
    let mut current_state = initial_state.clone();
    let mut snapshot_count = 0;

    let initial_size = serde_json::to_string(&initial_state)
        .unwrap_or_default()
        .len();

    let created = event_iter.header.created;

    // Add initial state as observation 0
    observations.push(ObservationInfo {
        id: "initial".to_string(),
        timestamp: created,
        created,
        change_count: 0,
        json_size: initial_size,
    });

    // Iterate through events
    while let Some(event) = event_iter.next() {
        match event {
            Event::Observe { observation_id, timestamp, change_count } => {
                observations.push(ObservationInfo {
                    id: observation_id,
                    timestamp,
                    created,
                    change_count,
                    json_size: 0, // Will be calculated after applying events
                });
            }
            Event::Add { path, value, .. } => {
                let _ = json_archive::apply_add(&mut current_state, &path, value);

                // Update the JSON size of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.json_size = serde_json::to_string(&current_state)
                            .unwrap_or_default()
                            .len();
                    }
                }
            }
            Event::Change { path, new_value, .. } => {
                let _ = json_archive::apply_change(&mut current_state, &path, new_value);

                // Update the JSON size of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.json_size = serde_json::to_string(&current_state)
                            .unwrap_or_default()
                            .len();
                    }
                }
            }
            Event::Remove { path, .. } => {
                let _ = json_archive::apply_remove(&mut current_state, &path);

                // Update the JSON size of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.json_size = serde_json::to_string(&current_state)
                            .unwrap_or_default()
                            .len();
                    }
                }
            }
            Event::Move { path, moves, .. } => {
                let _ = json_archive::apply_move(&mut current_state, &path, moves);

                // Update the JSON size of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.json_size = serde_json::to_string(&current_state)
                            .unwrap_or_default()
                            .len();
                    }
                }
            }
            Event::Snapshot { object, .. } => {
                current_state = object;
                snapshot_count += 1;

                // Update the JSON size of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    if last_obs.id != "initial" {
                        last_obs.json_size = serde_json::to_string(&current_state)
                            .unwrap_or_default()
                            .len();
                    }
                }
            }
        }
    }

    Ok((observations, snapshot_count))
}


fn format_timestamp(dt: &DateTime<Utc>) -> String {
    dt.format("%a %H:%M:%S %d-%b-%Y").to_string()
}

fn truncate_id(id: &str) -> String {
    if id.len() > 20 {
        format!("{}...", &id[..20])
    } else {
        id.to_string()
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} bytes", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
