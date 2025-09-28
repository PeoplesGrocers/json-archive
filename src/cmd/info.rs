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
use json_archive::{Diagnostic, DiagnosticCode, DiagnosticLevel};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
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

    let observations = match collect_observations(&flags.file) {
        Ok(obs) => obs,
        Err(diagnostics) => return diagnostics,
    };

    let file_size = match std::fs::metadata(&flags.file) {
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    };

    let snapshot_count = count_snapshots(&flags.file).unwrap_or(0);

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
        println!(
            "Total archive size: {} ({})",
            format_size(file_size),
            snapshot_text
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

fn collect_observations(file_path: &Path) -> Result<Vec<ObservationInfo>, Vec<Diagnostic>> {
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
    let initial_size = serde_json::to_string(&initial_state)
        .unwrap_or_default()
        .len();

    // Add initial state as observation 0
    observations.push(ObservationInfo {
        id: "initial".to_string(),
        timestamp: created,
        created,
        change_count: 0,
        json_size: initial_size,
    });

    let mut current_state = initial_state;

    // Parse events
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
                let change_count = arr[3].as_u64().unwrap_or(0) as usize;

                let timestamp: DateTime<Utc> = match timestamp_str.parse() {
                    Ok(dt) => dt,
                    Err(_) => continue,
                };

                observations.push(ObservationInfo {
                    id: obs_id,
                    timestamp,
                    created,
                    change_count,
                    json_size: 0, // Will be calculated after applying events
                });
            } else {
                // Apply the event to current_state for size calculation
                apply_event_to_state(&mut current_state, &arr);

                // Update the JSON size of the last observation
                if let Some(last_obs) = observations.last_mut() {
                    last_obs.json_size = serde_json::to_string(&current_state)
                        .unwrap_or_default()
                        .len();
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
        _ => {}
    }
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

fn count_snapshots(file_path: &Path) -> Result<usize, std::io::Error> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().starts_with('[') && line.contains("\"snapshot\"") {
            count += 1;
        }
    }

    Ok(count)
}
