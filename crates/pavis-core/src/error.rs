use std::fmt;

/// Canonical error codes for Pavis components.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    UnsupportedFeature,
    InvalidConfig,
    ValidationFailed,
    BackendIncompatible,
    UpstreamPoolFull,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ErrorCode::UnsupportedFeature => "ERR_UNSUPPORTED_FEATURE",
            ErrorCode::InvalidConfig => "ERR_INVALID_CONFIG",
            ErrorCode::ValidationFailed => "ERR_VALIDATION_FAILED",
            ErrorCode::BackendIncompatible => "ERR_BACKEND_INCOMPATIBLE",
            ErrorCode::UpstreamPoolFull => "ERR_UPSTREAM_POOL_FULL",
        };
        f.write_str(value)
    }
}

/// Structured context for errors.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorContext {
    pub feature: Option<String>,
    pub backend: Option<String>,
    pub field_path: Option<String>,
    pub constraint: Option<String>,
    pub upstream: Option<String>,
}

impl ErrorContext {
    pub fn with_field_path(mut self, field_path: impl Into<String>) -> Self {
        self.field_path = Some(field_path.into());
        self
    }

    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraint = Some(constraint.into());
        self
    }

    pub fn with_feature(mut self, feature: impl Into<String>) -> Self {
        self.feature = Some(feature.into());
        self
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    pub fn with_upstream(mut self, upstream: impl Into<String>) -> Self {
        self.upstream = Some(upstream.into());
        self
    }
}

/// Helper to build canonical field paths.
#[derive(Clone, Debug, Default)]
pub struct FieldPathBuilder {
    segments: String,
    started: bool,
}

impl FieldPathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root(mut self, root: impl AsRef<str>) -> Self {
        if self.started {
            panic!("FieldPathBuilder::root called twice");
        }
        self.segments.push_str(root.as_ref());
        self.started = true;
        self
    }

    pub fn field(mut self, field: impl AsRef<str>) -> Self {
        assert!(self.started, "FieldPathBuilder::field requires root");
        self.segments.push('.');
        self.segments.push_str(field.as_ref());
        self
    }

    pub fn index(mut self, index: usize) -> Self {
        assert!(self.started, "FieldPathBuilder::index requires root");
        use std::fmt::Write;
        write!(&mut self.segments, "[{index}]").expect("infallible");
        self
    }

    pub fn map_key(mut self, key: impl AsRef<str>) -> Self {
        assert!(self.started, "FieldPathBuilder::map_key requires root");
        self.segments.push('[');
        self.segments.push('"');
        for ch in key.as_ref().chars() {
            match ch {
                '"' => self.segments.push_str("\\\""),
                '\\' => self.segments.push_str("\\\\"),
                _ => self.segments.push(ch),
            }
        }
        self.segments.push('"');
        self.segments.push(']');
        self
    }

    pub fn finish(self) -> String {
        assert!(self.started, "FieldPathBuilder requires at least a root");
        self.segments
    }
}

/// Canonical Pavis error structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PavisError {
    pub code: ErrorCode,
    pub context: ErrorContext,
    pub message: String,
}

impl PavisError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            context: ErrorContext::default(),
            message: message.into(),
        }
    }

    pub fn with_context(mut self, f: impl FnOnce(ErrorContext) -> ErrorContext) -> Self {
        self.context = f(self.context);
        self
    }
}

impl fmt::Display for PavisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PavisError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_display_matches_expected_strings() {
        assert_eq!(
            ErrorCode::UnsupportedFeature.to_string(),
            "ERR_UNSUPPORTED_FEATURE"
        );
        assert_eq!(ErrorCode::InvalidConfig.to_string(), "ERR_INVALID_CONFIG");
        assert_eq!(
            ErrorCode::ValidationFailed.to_string(),
            "ERR_VALIDATION_FAILED"
        );
        assert_eq!(
            ErrorCode::BackendIncompatible.to_string(),
            "ERR_BACKEND_INCOMPATIBLE"
        );
        assert_eq!(
            ErrorCode::UpstreamPoolFull.to_string(),
            "ERR_UPSTREAM_POOL_FULL"
        );
    }

    #[test]
    fn error_context_builders_set_fields() {
        let ctx = ErrorContext::default()
            .with_field_path("telemetry.tracing")
            .with_constraint("must_be_enabled")
            .with_feature("tracing")
            .with_backend("otlp")
            .with_upstream("orders");

        assert_eq!(ctx.field_path.as_deref(), Some("telemetry.tracing"));
        assert_eq!(ctx.constraint.as_deref(), Some("must_be_enabled"));
        assert_eq!(ctx.feature.as_deref(), Some("tracing"));
        assert_eq!(ctx.backend.as_deref(), Some("otlp"));
        assert_eq!(ctx.upstream.as_deref(), Some("orders"));
    }

    #[test]
    fn field_path_builder_builds_and_escapes() {
        let path = FieldPathBuilder::new()
            .root("routes")
            .index(0)
            .field("matchers")
            .map_key("header\"value")
            .finish();
        assert_eq!(path, "routes[0].matchers[\"header\\\"value\"]");
    }

    #[test]
    fn field_path_builder_panics_without_root() {
        assert!(std::panic::catch_unwind(|| { FieldPathBuilder::new().field("routes") }).is_err());
    }

    #[test]
    fn pavis_error_display_includes_code_and_message() {
        let error = PavisError::new(ErrorCode::InvalidConfig, "missing telemetry block")
            .with_context(|ctx| ctx.with_field_path("telemetry"));
        assert_eq!(
            error.to_string(),
            "ERR_INVALID_CONFIG: missing telemetry block"
        );
        assert_eq!(error.context.field_path.as_deref(), Some("telemetry"));
    }
}
