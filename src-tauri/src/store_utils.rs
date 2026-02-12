use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub fn get_str(app: &AppHandle, key: &str, default: &str) -> String {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| default.to_string())
}

pub fn get_bool(app: &AppHandle, key: &str, default: bool) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

#[allow(dead_code)]
pub fn get_f64(app: &AppHandle, key: &str, default: f64) -> f64 {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_f64())
        .unwrap_or(default)
}

pub fn get_string_array(app: &AppHandle, key: &str) -> Vec<String> {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn set_bool(app: &AppHandle, key: &str, value: bool) {
    if let Ok(store) = app.store("settings.json") {
        store.set(key, serde_json::Value::Bool(value));
    }
}
