use pavis_core::{ErrorCode, PavisError};

pub fn invalid_config_error(
    message: impl Into<String>,
    field_path: Option<String>,
    constraint: Option<&str>,
) -> anyhow::Error {
    let err = PavisError::new(ErrorCode::InvalidConfig, message);
    let err = err.with_context(|ctx| {
        let mut ctx = ctx;
        if let Some(path) = field_path {
            ctx = ctx.with_field_path(path);
        }
        if let Some(code) = constraint {
            ctx = ctx.with_constraint(code.to_string());
        }
        ctx
    });
    anyhow::Error::new(err)
}
