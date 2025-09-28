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

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Fatal,
    Warning,
    Info,
}

impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLevel::Fatal => write!(f, "error"),
            DiagnosticLevel::Warning => write!(f, "warning"),
            DiagnosticLevel::Info => write!(f, "info"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCode {
    EmptyFile,
    MissingHeader,
    InvalidUtf8,
    TruncatedJson,

    MissingHeaderField,
    UnsupportedVersion,
    InvalidTimestamp,
    InvalidInitialState,

    InvalidEventJson,
    UnknownEventType,
    WrongFieldCount,
    WrongFieldType,

    NonExistentObservationId,
    DuplicateObservationId,

    ChangeCountMismatch,
    InvalidChangeCount,

    InvalidPointerSyntax,
    PathNotFound,
    InvalidArrayIndex,
    ArrayIndexOutOfBounds,
    ParentPathNotFound,

    TypeMismatch,
    OldValueMismatch,

    MoveOnNonArray,
    MoveIndexOutOfBounds,
    InvalidMoveIndex,

    SnapshotStateMismatch,
    SnapshotTimestampOrder,
}

impl DiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticCode::EmptyFile => "E001",
            DiagnosticCode::MissingHeader => "E002",
            DiagnosticCode::InvalidUtf8 => "E003",
            DiagnosticCode::TruncatedJson => "E004",

            DiagnosticCode::MissingHeaderField => "E010",
            DiagnosticCode::UnsupportedVersion => "E011",
            DiagnosticCode::InvalidTimestamp => "W012",
            DiagnosticCode::InvalidInitialState => "E013",

            DiagnosticCode::InvalidEventJson => "E020",
            DiagnosticCode::UnknownEventType => "W021",
            DiagnosticCode::WrongFieldCount => "E022",
            DiagnosticCode::WrongFieldType => "E023",

            DiagnosticCode::NonExistentObservationId => "E030",
            DiagnosticCode::DuplicateObservationId => "W031",

            DiagnosticCode::ChangeCountMismatch => "W040",
            DiagnosticCode::InvalidChangeCount => "E041",

            DiagnosticCode::InvalidPointerSyntax => "E050",
            DiagnosticCode::PathNotFound => "E051",
            DiagnosticCode::InvalidArrayIndex => "E052",
            DiagnosticCode::ArrayIndexOutOfBounds => "E053",
            DiagnosticCode::ParentPathNotFound => "E054",

            DiagnosticCode::TypeMismatch => "E060",
            DiagnosticCode::OldValueMismatch => "W061",

            DiagnosticCode::MoveOnNonArray => "E070",
            DiagnosticCode::MoveIndexOutOfBounds => "E071",
            DiagnosticCode::InvalidMoveIndex => "E072",

            DiagnosticCode::SnapshotStateMismatch => "W080",
            DiagnosticCode::SnapshotTimestampOrder => "W081",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            DiagnosticCode::EmptyFile => "Empty file",
            DiagnosticCode::MissingHeader => "Missing header",
            DiagnosticCode::InvalidUtf8 => "Invalid UTF-8 encoding",
            DiagnosticCode::TruncatedJson => "Truncated JSON",

            DiagnosticCode::MissingHeaderField => "Missing required header field",
            DiagnosticCode::UnsupportedVersion => "Unsupported version",
            DiagnosticCode::InvalidTimestamp => "Invalid timestamp",
            DiagnosticCode::InvalidInitialState => "Invalid initial state",

            DiagnosticCode::InvalidEventJson => "Invalid event JSON",
            DiagnosticCode::UnknownEventType => "Unknown event type",
            DiagnosticCode::WrongFieldCount => "Wrong field count",
            DiagnosticCode::WrongFieldType => "Wrong field type",

            DiagnosticCode::NonExistentObservationId => "Non-existent observation ID",
            DiagnosticCode::DuplicateObservationId => "Duplicate observation ID",

            DiagnosticCode::ChangeCountMismatch => "Change count mismatch",
            DiagnosticCode::InvalidChangeCount => "Invalid change count",

            DiagnosticCode::InvalidPointerSyntax => "Invalid JSON Pointer syntax",
            DiagnosticCode::PathNotFound => "Path not found",
            DiagnosticCode::InvalidArrayIndex => "Invalid array index",
            DiagnosticCode::ArrayIndexOutOfBounds => "Array index out of bounds",
            DiagnosticCode::ParentPathNotFound => "Parent path not found",

            DiagnosticCode::TypeMismatch => "Type mismatch",
            DiagnosticCode::OldValueMismatch => "Old value mismatch",

            DiagnosticCode::MoveOnNonArray => "Move operation on non-array",
            DiagnosticCode::MoveIndexOutOfBounds => "Move index out of bounds",
            DiagnosticCode::InvalidMoveIndex => "Invalid move index",

            DiagnosticCode::SnapshotStateMismatch => "Snapshot state mismatch",
            DiagnosticCode::SnapshotTimestampOrder => "Snapshot timestamp out of order",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub filename: Option<String>,
    pub line_number: Option<usize>,
    pub column: Option<usize>,
    pub level: DiagnosticLevel,
    pub code: DiagnosticCode,
    pub description: String,
    pub code_snippet: Option<String>,
    pub advice: Option<String>,
}

impl Diagnostic {
    pub fn new(level: DiagnosticLevel, code: DiagnosticCode, description: String) -> Self {
        Self {
            filename: None,
            line_number: None,
            column: None,
            level,
            code,
            description,
            code_snippet: None,
            advice: None,
        }
    }

    pub fn with_location(mut self, filename: String, line_number: usize) -> Self {
        self.filename = Some(filename);
        self.line_number = Some(line_number);
        self
    }

    pub fn with_column(mut self, column: usize) -> Self {
        self.column = Some(column);
        self
    }

    pub fn with_snippet(mut self, snippet: String) -> Self {
        self.code_snippet = Some(snippet);
        self
    }

    pub fn with_advice(mut self, advice: String) -> Self {
        self.advice = Some(advice);
        self
    }

    pub fn is_fatal(&self) -> bool {
        self.level == DiagnosticLevel::Fatal
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(filename), Some(line)) = (&self.filename, self.line_number) {
            if let Some(col) = self.column {
                write!(f, "{}:{}:{} - ", filename, line, col)?;
            } else {
                write!(f, "{}:{} - ", filename, line)?;
            }
        }

        writeln!(
            f,
            "{} {}: {}",
            self.level,
            self.code.as_str(),
            self.code.title()
        )?;
        writeln!(f)?;
        writeln!(f, "{}", self.description)?;

        if let Some(snippet) = &self.code_snippet {
            writeln!(f)?;
            writeln!(f, "{}", snippet)?;
        }

        if let Some(advice) = &self.advice {
            writeln!(f)?;
            writeln!(f, "{}", advice)?;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticCollector {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn has_fatal(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_fatal())
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }
}
