//! Error and source-location types shared by the parser, emitter, and Serde API.

use std::fmt;

/// A byte span plus one-based line and column for a YAML source location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// Zero-based byte offset where the span starts.
    pub start: usize,
    /// Zero-based byte offset where the span ends.
    pub end: usize,
    /// One-based source line for the start of the span.
    pub line: usize,
    /// One-based source column for the start of the span.
    pub column: usize,
}

/// A compact source location returned by [`Error::location`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    index: usize,
    line: usize,
    column: usize,
}

impl Location {
    /// Creates a new source location from a byte index and one-based line/column.
    pub fn new(index: usize, line: usize, column: usize) -> Self {
        Self {
            index,
            line,
            column,
        }
    }

    /// Returns the zero-based byte index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns the one-based source line.
    pub fn line(&self) -> usize {
        self.line
    }

    /// Returns the one-based source column.
    pub fn column(&self) -> usize {
        self.column
    }
}

impl Span {
    /// Creates a span from byte bounds and a one-based start line/column.
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Creates a zero-width span at the given byte offset and line/column.
    pub fn point(offset: usize, line: usize, column: usize) -> Self {
        Self::new(offset, offset, line, column)
    }
}

pub(crate) fn utf8_error_span(input: &[u8], error: std::str::Utf8Error) -> Span {
    let offset = error.valid_up_to();
    let mut line = 1usize;
    let mut column = 1usize;
    for byte in &input[..offset] {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Span::point(offset, line, column)
}

/// Additional source context associated with a primary diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedDiagnostic {
    /// Message for the related source location.
    pub message: String,
    /// Span for the related source location.
    pub span: Span,
}

/// Structured diagnostic payload for a YAML error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Primary diagnostic message.
    pub message: String,
    /// Primary source span.
    pub span: Span,
    /// Related diagnostics, such as the first occurrence of a duplicate key.
    pub related: Vec<RelatedDiagnostic>,
}

/// Error type returned by all public YAML APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    diagnostic: Diagnostic,
}

/// Result alias used by this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Creates an error with an optional primary span.
    pub fn new(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self {
            diagnostic: Diagnostic {
                message: message.into(),
                span: span.into().unwrap_or_default(),
                related: Vec::new(),
            },
        }
    }

    /// Creates an error with one related diagnostic.
    pub fn with_related(
        message: impl Into<String>,
        span: Span,
        related_message: impl Into<String>,
        related_span: Span,
    ) -> Self {
        Self {
            diagnostic: Diagnostic {
                message: message.into(),
                span,
                related: vec![RelatedDiagnostic {
                    message: related_message.into(),
                    span: related_span,
                }],
            },
        }
    }

    /// Returns the structured diagnostic payload.
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Returns the primary span, or [`Span::default`] for spanless errors.
    pub fn span(&self) -> Span {
        self.diagnostic.span
    }

    /// Returns the primary location when the error has a nonzero line/column.
    pub fn location(&self) -> Option<Location> {
        let span = self.span();
        (span.line > 0 && span.column > 0).then_some(Location::new(
            span.start,
            span.line,
            span.column,
        ))
    }

    /// Returns the one-based line of the primary diagnostic, if available.
    pub fn line(&self) -> Option<usize> {
        self.location().map(|location| location.line())
    }

    /// Returns the one-based column of the primary diagnostic, if available.
    pub fn column(&self) -> Option<usize> {
        self.location().map(|location| location.column())
    }

    pub(crate) fn with_span_if_missing(mut self, span: Span) -> Self {
        if self.location().is_none() {
            self.diagnostic.span = span;
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.location() {
            Some(location) => write!(
                f,
                "{} at line {}, column {}",
                self.diagnostic.message,
                location.line(),
                location.column()
            ),
            None => f.write_str(&self.diagnostic.message),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::new(msg.to_string(), Span::default())
    }
}

impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self::new(msg.to_string(), Span::default())
    }
}
