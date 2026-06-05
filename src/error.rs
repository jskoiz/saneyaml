//! Error and source-location types shared by the parser, emitter, and Serde API.
//!
//! ```rust
//! let input = "key: [unterminated\n";
//! let error = saneyaml::parse_str(input).unwrap_err();
//! assert!(error.location().is_some());
//! assert!(error.render_source(input).to_string().contains('^'));
//! ```

use std::fmt;

/// Broad, stable category for a YAML error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// YAML syntax or structural parse failure.
    Syntax,
    /// Input bytes are not valid for the requested text encoding.
    Encoding,
    /// Reader or writer I/O failure.
    Io,
    /// Configured input, nesting, or expansion limit was exceeded.
    Limit,
    /// Anchor, alias, or other reference resolution failure.
    Reference,
    /// Duplicate mapping key failure.
    DuplicateKey,
    /// Serde data-model or typed value mismatch.
    Data,
    /// Requested operation is outside the implemented YAML surface.
    Unsupported,
    /// Source-preserving lossless edit or graph failure.
    Lossless,
    /// Error category was not classified more narrowly.
    Other,
}

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

/// Path to a value inside a YAML document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorPath {
    segments: Vec<ErrorPathSegment>,
}

impl ErrorPath {
    /// Creates an empty path.
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Creates a path from ordered path segments.
    pub fn from_segments(segments: Vec<ErrorPathSegment>) -> Self {
        Self { segments }
    }

    /// Returns the ordered path segments.
    pub fn segments(&self) -> &[ErrorPathSegment] {
        &self.segments
    }

    /// Returns true when this path has no segments.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub(crate) fn prepend(&mut self, segment: ErrorPathSegment) {
        self.segments.insert(0, segment);
    }
}

impl fmt::Display for ErrorPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, segment) in self.segments.iter().enumerate() {
            match segment {
                ErrorPathSegment::Field(field) | ErrorPathSegment::Key(field)
                    if is_plain_path_key(field) =>
                {
                    if index > 0 {
                        f.write_str(".")?;
                    }
                    f.write_str(field)?;
                }
                ErrorPathSegment::Field(field) | ErrorPathSegment::Key(field) => {
                    write!(f, "[\"{}\"]", EscapedPathString(field))?;
                }
                ErrorPathSegment::Index(index) => write!(f, "[{index}]")?,
                ErrorPathSegment::ScalarKey(key) => write!(f, "[{key}]")?,
                ErrorPathSegment::ComplexKey => f.write_str("[{complex key}]")?,
            }
        }
        Ok(())
    }
}

/// One segment of an [`ErrorPath`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorPathSegment {
    /// Named Serde struct field.
    Field(String),
    /// String-like YAML mapping key.
    Key(String),
    /// Zero-based YAML sequence index.
    Index(usize),
    /// Scalar YAML mapping key rendered as diagnostic text.
    ScalarKey(String),
    /// Complex YAML mapping key that has no compact scalar diagnostic form.
    ComplexKey,
}

fn is_plain_path_key(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, 'A'..='Z' | 'a'..='z' | '_')
        && chars.all(|ch| matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-'))
}

struct EscapedPathString<'a>(&'a str);

impl fmt::Display for EscapedPathString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for ch in self.0.chars() {
            match ch {
                '\\' => f.write_str("\\\\")?,
                '"' => f.write_str("\\\"")?,
                '\n' => f.write_str("\\n")?,
                '\r' => f.write_str("\\r")?,
                '\t' => f.write_str("\\t")?,
                ch if ch.is_control() => write!(f, "\\u{:04X}", ch as u32)?,
                ch => f.write_str(ch.encode_utf8(&mut [0; 4]))?,
            }
        }
        Ok(())
    }
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
    /// Broad diagnostic category.
    pub category: ErrorCategory,
    /// Optional YAML document index for stream diagnostics.
    pub document_index: Option<usize>,
    /// Optional path to the in-document value.
    pub path: Option<ErrorPath>,
}

/// Error type returned by all public YAML APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    diagnostic: Box<Diagnostic>,
}

/// Result alias used by this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Creates an error with an optional primary span.
    pub fn new(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Other)
    }

    /// Creates an error with an explicit category.
    pub fn with_category(
        message: impl Into<String>,
        span: impl Into<Option<Span>>,
        category: ErrorCategory,
    ) -> Self {
        Self {
            diagnostic: Box::new(Diagnostic {
                message: message.into(),
                span: span.into().unwrap_or_default(),
                related: Vec::new(),
                category,
                document_index: None,
                path: None,
            }),
        }
    }

    pub(crate) fn data(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Data)
    }

    pub(crate) fn syntax(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Syntax)
    }

    pub(crate) fn encoding(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Encoding)
    }

    pub(crate) fn io(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Io)
    }

    pub(crate) fn limit(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Limit)
    }

    pub(crate) fn reference(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self::with_category(message, span, ErrorCategory::Reference)
    }

    /// Creates an error with one related diagnostic.
    pub fn with_related(
        message: impl Into<String>,
        span: Span,
        related_message: impl Into<String>,
        related_span: Span,
    ) -> Self {
        Self {
            diagnostic: Box::new(Diagnostic {
                message: message.into(),
                span,
                related: vec![RelatedDiagnostic {
                    message: related_message.into(),
                    span: related_span,
                }],
                category: ErrorCategory::Other,
                document_index: None,
                path: None,
            }),
        }
    }

    pub(crate) fn with_related_category(
        message: impl Into<String>,
        span: Span,
        related_message: impl Into<String>,
        related_span: Span,
        category: ErrorCategory,
    ) -> Self {
        let mut error = Self::with_related(message, span, related_message, related_span);
        error.diagnostic.category = category;
        error
    }

    /// Returns the structured diagnostic payload.
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Returns the primary span, or [`Span::default`] for spanless errors.
    pub fn span(&self) -> Span {
        self.diagnostic.span
    }

    /// Returns the broad diagnostic category.
    pub fn category(&self) -> ErrorCategory {
        self.diagnostic.category
    }

    /// Returns the zero-based document index for stream diagnostics.
    pub fn document_index(&self) -> Option<usize> {
        self.diagnostic.document_index
    }

    /// Returns the in-document path for Serde diagnostics.
    pub fn path(&self) -> Option<&ErrorPath> {
        self.diagnostic.path.as_ref()
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

    /// Renders this error with source context and caret markers.
    pub fn render_source<'a>(&'a self, source: &'a str) -> SourceDiagnostic<'a> {
        self.render_source_with_options(source, SourceRenderOptions::default())
    }

    /// Renders this error with source context and custom options.
    pub fn render_source_with_options<'a>(
        &'a self,
        source: &'a str,
        options: SourceRenderOptions,
    ) -> SourceDiagnostic<'a> {
        self.diagnostic.render_source_with_options(source, options)
    }

    pub(crate) fn with_span_if_missing(mut self, span: Span) -> Self {
        if self.location().is_none() {
            self.diagnostic.span = span;
        }
        self
    }

    pub(crate) fn with_document_index(mut self, index: usize) -> Self {
        if index > 0 {
            self.diagnostic.document_index.get_or_insert(index);
        }
        self
    }

    pub(crate) fn with_path_segment_if_empty(mut self, segment: ErrorPathSegment) -> Self {
        if self
            .diagnostic
            .path
            .as_ref()
            .is_none_or(ErrorPath::is_empty)
        {
            self.diagnostic.path = Some(ErrorPath::from_segments(vec![segment]));
        }
        self
    }

    pub(crate) fn prepend_path_segment(mut self, segment: ErrorPathSegment) -> Self {
        match &mut self.diagnostic.path {
            Some(path) => path.prepend(segment),
            None => self.diagnostic.path = Some(ErrorPath::from_segments(vec![segment])),
        }
        self
    }
}

impl Diagnostic {
    /// Returns the broad diagnostic category.
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the zero-based document index for stream diagnostics.
    pub fn document_index(&self) -> Option<usize> {
        self.document_index
    }

    /// Returns the in-document path for Serde diagnostics.
    pub fn path(&self) -> Option<&ErrorPath> {
        self.path.as_ref()
    }

    /// Renders this diagnostic with source context and caret markers.
    pub fn render_source<'a>(&'a self, source: &'a str) -> SourceDiagnostic<'a> {
        self.render_source_with_options(source, SourceRenderOptions::default())
    }

    /// Renders this diagnostic with source context and custom options.
    pub fn render_source_with_options<'a>(
        &'a self,
        source: &'a str,
        options: SourceRenderOptions,
    ) -> SourceDiagnostic<'a> {
        SourceDiagnostic {
            diagnostic: self,
            source,
            options,
        }
    }
}

/// Options for source-context diagnostic rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceRenderOptions {
    /// Number of source lines to render before and after the primary line.
    ///
    /// The default `0` preserves compact rendering with only the diagnostic
    /// line. Nonzero values include up to that many neighboring source lines on
    /// each side for both primary and related spans.
    pub context_lines: usize,
}

/// Display wrapper for explicit source-context diagnostic rendering.
#[derive(Clone, Copy, Debug)]
pub struct SourceDiagnostic<'a> {
    diagnostic: &'a Diagnostic,
    source: &'a str,
    options: SourceRenderOptions,
}

impl fmt::Display for SourceDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match diagnostic_location(self.diagnostic) {
            Some(location) => write!(
                f,
                "{} at line {}, column {}",
                self.diagnostic.message,
                location.line(),
                location.column()
            )?,
            None => f.write_str(&self.diagnostic.message)?,
        }
        if let Some(path) = &self.diagnostic.path
            && !path.is_empty()
        {
            write!(f, "\npath: {path}")?;
        }
        if let Some(index) = self.diagnostic.document_index {
            write!(f, "\ndocument: {index}")?;
        }
        render_span_block(f, self.source, self.diagnostic.span, self.options)?;
        for related in &self.diagnostic.related {
            write!(f, "\n{}", related.message)?;
            render_span_block(f, self.source, related.span, self.options)?;
        }
        Ok(())
    }
}

fn diagnostic_location(diagnostic: &Diagnostic) -> Option<Location> {
    let span = diagnostic.span;
    (span.line > 0 && span.column > 0).then_some(Location::new(span.start, span.line, span.column))
}

fn render_span_block(
    f: &mut fmt::Formatter<'_>,
    source: &str,
    span: Span,
    options: SourceRenderOptions,
) -> fmt::Result {
    if span.line == 0 || span.column == 0 || span.start > source.len() {
        return Ok(());
    }
    let Some((line_start, line_end, _)) = line_bounds(source, span.start) else {
        return Ok(());
    };
    let line_number = span.line;
    let context_start = line_number.saturating_sub(options.context_lines).max(1);
    let context_end = line_number.saturating_add(options.context_lines);
    let width = context_end.to_string().len();
    writeln!(f)?;
    writeln!(f, "{:>width$} |", "", width = width)?;
    let caret_start = floor_char_boundary(source, span.start.clamp(line_start, line_end));
    let caret_end = floor_char_boundary(source, span.end.clamp(caret_start, line_end));
    let mut rendered_line = false;
    for current_line in context_start..=context_end {
        let Some((current_start, _, line_text)) =
            line_bounds_for_line(source, current_line, current_line == line_number)
        else {
            continue;
        };
        if rendered_line {
            writeln!(f)?;
        }
        write!(f, "{current_line:>width$} | {line_text}", width = width)?;
        if current_line == line_number {
            writeln!(f)?;
            write!(f, "{:>width$} | ", "", width = width)?;
            for byte in source.as_bytes()[current_start..caret_start]
                .iter()
                .copied()
            {
                if byte == b'\t' {
                    f.write_str("\t")?;
                } else {
                    f.write_str(" ")?;
                }
            }
            let caret_count = caret_end.saturating_sub(caret_start).max(1);
            for _ in 0..caret_count {
                f.write_str("^")?;
            }
        }
        rendered_line = true;
    }
    Ok(())
}

fn line_bounds(source: &str, offset: usize) -> Option<(usize, usize, &str)> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = source[..offset]
        .rfind('\n')
        .map_or(0, |index| index.saturating_add(1));
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    Some((line_start, line_end, &source[line_start..line_end]))
}

fn line_bounds_for_line(
    source: &str,
    target_line: usize,
    include_trailing_empty_line: bool,
) -> Option<(usize, usize, &str)> {
    if target_line == 0 {
        return None;
    }
    let mut line = 1usize;
    let mut start = 0usize;
    for part in source.split_inclusive('\n') {
        let end = start + part.len();
        let text_end = end.saturating_sub(usize::from(part.ends_with('\n')));
        if line == target_line {
            return Some((start, text_end, &source[start..text_end]));
        }
        start = end;
        line += 1;
    }
    if include_trailing_empty_line && source.ends_with('\n') && line == target_line {
        return Some((source.len(), source.len(), ""));
    }
    None
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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
        Self::data(msg.to_string(), Span::default())
    }

    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        let message = if expected.is_empty() {
            format!("unknown field `{field}`")
        } else {
            format!(
                "unknown field `{}`, expected one of {}",
                field,
                expected
                    .iter()
                    .map(|field| format!("`{field}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        Self::data(message, Span::default())
            .with_path_segment_if_empty(ErrorPathSegment::Field(field.to_string()))
    }

    fn missing_field(field: &'static str) -> Self {
        Self::data(format!("missing field `{field}`"), Span::default())
            .with_path_segment_if_empty(ErrorPathSegment::Field(field.to_string()))
    }

    fn duplicate_field(field: &'static str) -> Self {
        Self::data(format!("duplicate field `{field}`"), Span::default())
            .with_path_segment_if_empty(ErrorPathSegment::Field(field.to_string()))
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
