use std::path::PathBuf;

use tauri::AppHandle;

pub fn resolve_preferred_js_runtime(app: &AppHandle) -> Option<(String, PathBuf)> {
    // Prefer Deno if available
    if let Some(path) = crate::deps::specs::deno::resolve_deno_path(app) {
        return Some(("deno".to_string(), path));
    }

    // Then QuickJS (qjs)
    if let Some(path) = crate::deps::specs::quickjs::resolve_quickjs_path(app) {
        return Some(("qjs".to_string(), path));
    }

    None
}
