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

//! File type detection for JSON archives.
//!
//! This module exists to support ergonomic command-line usage without requiring
//! `--archive=filename` flags. The goal is to infer intent just from filenames:
//!
//! - `json-archive data.json.archive data.json` -> append data.json to existing archive
//! - `json-archive data.json` -> create new archive from data.json
//! - `json-archive data.json.archive.tmp foo.json bar.json` -> append to archive with .tmp suffix
//!
//! Design choice by @nobody. No user requests for this, just seemed nice.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Detects if a file is a JSON archive by checking file extension or inspecting the header.
///
/// Detection strategy:
/// 1. Check if filename ends with .json.archive
/// 2. Inspect first line for type field as first key with value "@peoplesgrocers/json-archive"
///
/// Strategy 2 was added by @nobody based on frustration with the Elm compiler,
/// which requires specific file extensions (like .js) while build systems often generate
/// temporary files with arbitrary suffixes like .tmp. @nobody thought it would be nice if the CLI
/// was robust enough to handle this.
///
///
/// The magic value "@peoplesgrocers/json-archive" in the type field works as a file
/// signature for cases where the extension isn't what we expect. Not requested by anyone,
/// just anticipating potential tooling conflicts.
pub fn is_json_archive<P: AsRef<Path>>(path: P) -> Result<bool, std::io::Error> {
    let path = path.as_ref();

    if let Some(filename) = path.file_name() {
        if let Some(filename_str) = filename.to_str() {
            if filename_str.ends_with(".json.archive") {
                return Ok(true);
            }
        }
    }

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();

    match reader.read_line(&mut first_line) {
        Ok(0) => return Ok(false), // Empty file
        Ok(_) => {
            // Try to parse as JSON and check if it has our type field as the first key
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&first_line) {
                if let Some(obj) = value.as_object() {
                    // Check if the first key is "type" with our expected value
                    // Note: serde_json::Map preserves insertion order
                    if let Some((first_key, first_value)) = obj.iter().next() {
                        if first_key == "type" {
                            if let Some(type_str) = first_value.as_str() {
                                return Ok(type_str == "@peoplesgrocers/json-archive");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => return Err(e),
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_detect_by_json_archive_extension() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".json.archive")?;
        writeln!(temp_file, r#"{{"some": "json"}}"#)?;
        temp_file.flush()?;

        assert!(is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_detect_by_type_field() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".weird-extension")?;
        writeln!(
            temp_file,
            r#"{{"type":"@peoplesgrocers/json-archive","version":1}}"#
        )?;
        temp_file.flush()?;

        assert!(is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_detect_by_type_field_with_tmp_extension() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".json.tmp")?;
        writeln!(
            temp_file,
            r#"{{"type":"@peoplesgrocers/json-archive","version":1}}"#
        )?;
        temp_file.flush()?;

        assert!(is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_not_archive_regular_json() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".json")?;
        writeln!(temp_file, r#"{{"some": "json"}}"#)?;
        temp_file.flush()?;

        assert!(!is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_not_archive_wrong_type_field() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".tmp")?;
        writeln!(temp_file, r#"{{"type":"something-else","version":1}}"#)?;
        temp_file.flush()?;

        assert!(!is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_not_archive_type_not_first_field() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".tmp")?;
        // Use a key that comes after "type" alphabetically to ensure it's first
        writeln!(
            temp_file,
            r#"{{"version":1,"zzz":"@peoplesgrocers/json-archive"}}"#
        )?;
        temp_file.flush()?;

        // This should NOT be detected as an archive since the type field doesn't exist
        assert!(!is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_not_archive_empty_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::with_suffix(".json")?;

        assert!(!is_json_archive(temp_file.path())?);
        Ok(())
    }

    #[test]
    fn test_not_archive_invalid_json() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::with_suffix(".tmp")?;
        writeln!(temp_file, "not valid json")?;
        temp_file.flush()?;

        assert!(!is_json_archive(temp_file.path())?);
        Ok(())
    }
}
