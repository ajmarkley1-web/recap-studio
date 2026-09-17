//! Progress and log events pushed to the UI.
//!
//! Long jobs (ingest, analyze, narrate, render) run on background tasks and
//! report through here, so the frontend never has to poll.

use serde::Serialize;
use tauri::{AppHandle, Emitter as _};

pub const PROGRESS_EVENT: &str = "recap://progress";
pub const LOG_EVENT: &str = "recap://log";

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// Which pipeline this belongs to: `ingest`, `analyze`, `script`, `narrate`, `render`.
    pub job: String,
    /// Short name of the current stage, shown as the active step in the UI.
    pub stage: String,
    pub current: usize,
    pub total: usize,
    pub message: String,
    pub done: bool,
    pub error: Option<String>,
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub level: String,
    pub text: String,
    pub at: String,
    pub project_id: String,
}

#[derive(Clone)]
pub struct Emitter {
    app: AppHandle,
    job: String,
    project_id: String,
}

impl Emitter {
    pub fn new(app: AppHandle, job: &str, project_id: &str) -> Self {
        Self {
            app,
            job: job.to_string(),
            project_id: project_id.to_string(),
        }
    }

    pub fn progress(&self, stage: &str, current: usize, total: usize, message: impl Into<String>) {
        let payload = Progress {
            job: self.job.clone(),
            stage: stage.to_string(),
            current,
            total,
            message: message.into(),
            done: false,
            error: None,
            project_id: self.project_id.clone(),
        };
        let _ = self.app.emit(PROGRESS_EVENT, payload);
    }

    pub fn stage(&self, stage: &str, message: impl Into<String>) {
        self.progress(stage, 0, 0, message);
    }

    pub fn done(&self, message: impl Into<String>) {
        let payload = Progress {
            job: self.job.clone(),
            stage: "done".into(),
            current: 1,
            total: 1,
            message: message.into(),
            done: true,
            error: None,
            project_id: self.project_id.clone(),
        };
        let _ = self.app.emit(PROGRESS_EVENT, payload);
    }

    pub fn failed(&self, error: impl Into<String>) {
        let err = error.into();
        self.log("error", err.clone());
        let payload = Progress {
            job: self.job.clone(),
            stage: "failed".into(),
            current: 0,
            total: 0,
            message: "Job failed".into(),
            done: true,
            error: Some(err),
            project_id: self.project_id.clone(),
        };
        let _ = self.app.emit(PROGRESS_EVENT, payload);
    }

    pub fn log(&self, level: &str, text: impl Into<String>) {
        let line = LogLine {
            level: level.to_string(),
            text: text.into(),
            at: crate::util::now_iso(),
            project_id: self.project_id.clone(),
        };
        tracing::info!("[{}] {}", self.job, line.text);
        let _ = self.app.emit(LOG_EVENT, line);
    }

    pub fn info(&self, text: impl Into<String>) {
        self.log("info", text);
    }

    pub fn warn(&self, text: impl Into<String>) {
        self.log("warn", text);
    }
}
