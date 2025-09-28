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

use std::path::PathBuf;

xflags::xflags! {
    cmd json-archive {
        default cmd create {
            /// Input JSON files in chronological order (first file determines default output name)
            repeated inputs: PathBuf

            /// Output archive file path (defaults to first input + .json.archive)
            optional -o, --output output: PathBuf

            /// Insert snapshot every N observations (optional)
            optional -s, --snapshot-interval snapshot_interval: usize

            /// Source identifier for archive metadata
            optional --source source: String
        }

        cmd info {
            /// Archive file to show information about
            required file: PathBuf

            /// Output format: human-readable (default) or json
            optional --output output: String
        }

        cmd state {
            /// Archive file to read state from
            required file: PathBuf

            /// Get state at specific observation ID
            optional --id id: String

            /// Get state at Nth observation in file order (not chronological)
            optional --index index: usize

            /// Get state as of this timestamp (most recent observation <= timestamp)
            optional --as-of as_of: String

            /// Get state right before this timestamp (most recent observation < timestamp)
            optional --before before: String

            /// Get state after this timestamp (earliest observation > timestamp)
            optional --after after: String

            /// Get latest state by timestamp (default if no other flags specified)
            optional --latest latest: bool
        }
    }
}
