use std::path::PathBuf;

use tauri::AppHandle;

pub fn resolve_available_js_runtimes(app: &AppHandle) -> Vec<(String, PathBuf)> {
    let mut runtimes = Vec::new();

    if let Some(path) = crate::deps::specs::deno::resolve_deno_path(app) {
        runtimes.push(("deno".to_string(), path));
    }

    if let Some(path) = crate::deps::specs::quickjs::resolve_quickjs_path(app) {
        runtimes.push(("quickjs".to_string(), path));
    }

    runtimes
}
