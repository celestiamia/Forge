//! Error reporting for the Forge semantic analyzer.
//!
//! Diagnostics carry an optional source location so that callers can report
//! file/line/column information when it is available.  The AST produced by the
//! current parser does not include source spans, so locations default to
//! "unknown" and only the message is populated.

use std::fmt;

/// A source location: file path plus 1-based line and column numbers.
///
/// All fields are optional because the AST may not carry span information.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Loc {
    pub file: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
}

impl Loc {
    /// Create a fully populated location.
    #[allow(dead_code)]
    pub fn new(file: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            file: Some(file.into()),
            line: Some(line),
            col: Some(col),
        }
    }

    /// Create a location with only a file path.
    pub fn with_file(file: impl Into<String>) -> Self {
        Self {
            file: Some(file.into()),
            line: None,
            col: None,
        }
    }

    /// Create an unknown location.
    pub fn unknown() -> Self {
        Self::default()
    }
}

impl fmt::Display for Loc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line, self.col) {
            (Some(file), Some(line), Some(col)) => write!(f, "{}:{}:{}", file, line, col),
            (Some(file), Some(line), None) => write!(f, "{}:{}", file, line),
            (Some(file), None, _) => write!(f, "{}", file),
            (None, Some(line), Some(col)) => write!(f, "{}:{}", line, col),
            (None, Some(line), None) => write!(f, "{}", line),
            (None, None, Some(col)) => write!(f, "col:{}", col),
            (None, None, None) => write!(f, "<unknown>"),
        }
    }
}

/// A semantic error with an optional source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub loc: Loc,
    pub message: String,
}

impl Error {
    pub fn new(loc: Loc, message: impl Into<String>) -> Self {
        Self {
            loc,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.loc, self.message)
    }
}

impl std::error::Error for Error {}
