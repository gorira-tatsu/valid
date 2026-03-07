//! Structured diagnostics shared by frontend and later execution layers.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub source: String,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            source: "<memory>".to_string(),
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSegment {
    FrontendParse,
    FrontendResolve,
    FrontendTypecheck,
    FrontendLowering,
    KernelEval,
    KernelTransition,
    EngineSearch,
}

impl DiagnosticSegment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FrontendParse => "frontend.parse",
            Self::FrontendResolve => "frontend.resolve",
            Self::FrontendTypecheck => "frontend.typecheck",
            Self::FrontendLowering => "frontend.lowering",
            Self::KernelEval => "kernel.eval",
            Self::KernelTransition => "kernel.transition",
            Self::EngineSearch => "engine.search",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    ParseError,
    NameResolutionError,
    TypecheckError,
    UnsupportedExpr,
    InvalidTransitionUpdate,
    EvalError,
    InvalidState,
    SearchError,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ParseError => "PARSE_ERROR",
            Self::NameResolutionError => "NAME_RESOLUTION_ERROR",
            Self::TypecheckError => "TYPECHECK_ERROR",
            Self::UnsupportedExpr => "UNSUPPORTED_EXPR",
            Self::InvalidTransitionUpdate => "INVALID_TRANSITION_UPDATE",
            Self::EvalError => "EVAL_ERROR",
            Self::InvalidState => "INVALID_STATE",
            Self::SearchError => "SEARCH_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub error_code: ErrorCode,
    pub segment: DiagnosticSegment,
    pub severity: Severity,
    pub message: String,
    pub primary_span: Option<Span>,
    pub conflicts: Vec<String>,
    pub help: Vec<String>,
    pub best_practices: Vec<String>,
}

impl Diagnostic {
    pub fn new(
        error_code: ErrorCode,
        segment: DiagnosticSegment,
        message: impl Into<String>,
    ) -> Self {
        Self {
            error_code,
            segment,
            severity: Severity::Error,
            message: message.into(),
            primary_span: None,
            conflicts: Vec::new(),
            help: Vec::new(),
            best_practices: Vec::new(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.primary_span = Some(span);
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    pub fn with_best_practice(mut self, hint: impl Into<String>) -> Self {
        self.best_practices.push(hint.into());
        self
    }

    pub fn with_conflict(mut self, conflict: impl Into<String>) -> Self {
        self.conflicts.push(conflict.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}]: {}",
            self.error_code.as_str(),
            self.segment.as_str(),
            self.message
        )
    }
}
