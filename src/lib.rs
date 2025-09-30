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

pub mod archive;
pub mod detection;
pub mod diagnostics;
pub mod diff;
pub mod event_deserialize;
pub mod events;
pub mod flags;
pub mod pointer;
pub mod reader;

pub use archive::{
    append_to_archive, create_archive_from_files, default_output_filename, ArchiveBuilder, ArchiveWriter,
};
pub use detection::is_json_archive;
pub use diagnostics::{Diagnostic, DiagnosticCode, DiagnosticCollector, DiagnosticLevel};
pub use events::{Event, Header, Observation};
pub use pointer::JsonPointer;
pub use reader::{apply_add, apply_change, apply_move, apply_remove, ArchiveReader, ReadMode, ReadResult};
