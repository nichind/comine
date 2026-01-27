use tauri::{AppHandle, Emitter};

use crate::types::InstallProgress;

#[derive(Clone)]
pub struct ProgressEmitter {
    app: AppHandle,
    event_name: &'static str,
}

impl ProgressEmitter {
    pub fn new(app: &AppHandle, event_name: &'static str) -> Self {
        Self {
            app: app.clone(),
            event_name,
        }
    }

    pub fn emit(&self, progress: InstallProgress) {
        let _ = self.app.emit(self.event_name, progress);
    }

    #[allow(dead_code)]
    pub fn stage(&self, stage: &'static str, progress: u8, message: impl Into<String>) {
        self.emit(InstallProgress {
            stage: stage.to_string(),
            progress,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: message.into(),
        });
    }
}
