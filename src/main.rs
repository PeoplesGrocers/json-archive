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

use json_archive::{
    append_to_archive, create_archive_from_files, default_output_filename, is_json_archive, Diagnostic,
    DiagnosticCode, DiagnosticLevel,
};
use std::path::Path;
use std::process;

mod cmd;
mod flags;

fn main() {
    let flags = flags::JsonArchive::from_env_or_exit();

    let diagnostics = run(flags);

    for diagnostic in &diagnostics {
        eprintln!("{}", diagnostic);
    }

    let has_fatal = diagnostics.iter().any(|d| d.is_fatal());
    if has_fatal {
        process::exit(1);
    }
}

fn run(flags: flags::JsonArchive) -> Vec<Diagnostic> {
    match flags.subcommand {
        flags::JsonArchiveCmd::Create(create_flags) => create_archive(&create_flags),
        flags::JsonArchiveCmd::Info(info_flags) => cmd::info::run(&info_flags),
        flags::JsonArchiveCmd::State(state_flags) => cmd::state::run(&state_flags),
    }
}

fn create_archive(flags: &flags::Create) -> Vec<Diagnostic> {
    if flags.inputs.is_empty() {
        return vec![Diagnostic::new(
            DiagnosticLevel::Fatal,
            DiagnosticCode::MissingHeaderField,
            "I need at least one JSON file to create an archive, but you didn't provide any."
                .to_string(),
        )
        .with_advice(
            "Usage: json-archive <file1.json> [file2.json ...]\n\n\
                 The first file will be used as the initial state, and subsequent files \
                 will be compared to generate change events."
                .to_string(),
        )];
    }

    let output_path = match &flags.output {
        Some(path) => path.clone(),
        None => default_output_filename(&flags.inputs[0]),
    };

    let mut diagnostics = Vec::new();
    for input_path in &flags.inputs {
        if !Path::new(input_path).exists() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::PathNotFound,
                    format!("I couldn't find the input file: {}", input_path.display()),
                )
                .with_advice(
                    "Make sure the file path is correct and the file exists. \
                     Check for typos in the filename."
                        .to_string(),
                ),
            );
        }
    }

    if !diagnostics.is_empty() {
        return diagnostics;
    }

    let first_is_archive = match is_json_archive(&flags.inputs[0]) {
        Ok(is_archive) => is_archive,
        Err(e) => {
            return vec![Diagnostic::new(
                DiagnosticLevel::Fatal,
                DiagnosticCode::PathNotFound,
                format!("I couldn't check if the first file is an archive: {}", e),
            )];
        }
    };

    if first_is_archive {
        println!("First input appears to be a JSON archive file");
        if flags.inputs.len() == 1 {
            return vec![
                Diagnostic::new(
                    DiagnosticLevel::Fatal,
                    DiagnosticCode::MissingHeaderField,
                    "I found that the first input is already an archive file, but you didn't provide any additional JSON files to append.".to_string()
                )
                .with_advice(
                    "If you want to append to an archive, provide additional JSON files:\n\
                     json-archive existing.json.archive new1.json new2.json"
                        .to_string()
                )
            ];
        }

        return append_to_archive(&flags.inputs[0], &flags.inputs[1..], &output_path, flags.source.clone(), flags.snapshot_interval);
    }

    println!("Creating archive: {}", output_path.display());
    println!("Input files: {:?}", flags.inputs);

    if let Some(interval) = flags.snapshot_interval {
        println!("Snapshot interval: every {} observations", interval);
    }

    if let Some(ref source) = flags.source {
        println!("Source: {}", source);
    }

    match create_archive_from_files(
        &flags.inputs,
        output_path.clone(),
        flags.source.clone(),
        flags.snapshot_interval,
    ) {
        Ok(()) => {
            println!("Archive created successfully: {}", output_path.display());
            Vec::new()
        }
        Err(diagnostics) => diagnostics,
    }
}
