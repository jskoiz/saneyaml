use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    index: usize,
    line: usize,
    column: usize,
}

impl Location {
    pub fn new(index: usize, line: usize, column: usize) -> Self {
        Self {
            index,
            line,
            column,
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedDiagnostic {
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub related: Vec<RelatedDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    diagnostic: Diagnostic,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self {
            diagnostic: Diagnostic {
                message: message.into(),
                span: span.into().unwrap_or_default(),
                related: Vec::new(),
            },
        }
    }

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

    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    pub fn span(&self) -> Span {
        self.diagnostic.span
    }

    pub fn location(&self) -> Option<Location> {
        let span = self.span();
        (span.line > 0 && span.column > 0).then_some(Location::new(
            span.start,
            span.line,
            span.column,
        ))
    }

    pub fn line(&self) -> Option<usize> {
        self.location().map(|location| location.line())
    }

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
