use serde::Serialize;

/// Error type returned to the frontend. Every command returns `Result<T, AppError>`
/// so the UI can show something specific instead of "something went wrong".
#[derive(Debug, Serialize)]
pub struct AppError {
    pub message: String,
    /// Short machine-readable tag the UI can branch on.
    pub kind: String,
    /// Optional actionable next step shown under the error in the UI.
    pub hint: Option<String>,
}

impl AppError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: kind.to_string(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        // Surface the whole chain; the root cause is usually the useful part.
        let mut message = err.to_string();
        let mut source = err.source();
        while let Some(cause) = source {
            message.push_str(&format!(" -> {cause}"));
            source = cause.source();
        }
        AppError::new("internal", message)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::new("io", err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::new("json", err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
