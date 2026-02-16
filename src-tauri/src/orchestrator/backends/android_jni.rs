use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::orchestrator::types::*;

use super::ytdlp::common::{parse_raw_progress, parse_ytdlp_line_event, YtdlpEvent};

macro_rules! jni_string {
    ($env:expr, $jstr:expr) => {
        match $env.get_string(&$jstr) {
            Ok(s) => String::from(s),
            Err(_) => return,
        }
    };
    ($env:expr, $jstr:expr, $default:expr) => {
        match $env.get_string(&$jstr) {
            Ok(s) => String::from(s),
            Err(_) => $default,
        }
    };
}
pub(crate) use jni_string;

#[derive(Debug, Clone)]
pub enum AndroidEvent {
    RawProgressLine(String),
    ResolveOutput(String),
    Metadata(YtdlpEvent),
    FileOutput(String),
    Completed {
        output_path: String,
        title: Option<String>,
    },
    Failed(String),
    Cancelled,
    Paused,
}

pub static ANDROID_LISTENERS: LazyLock<
    Mutex<HashMap<String, mpsc::UnboundedSender<AndroidEvent>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock_listeners(
) -> std::sync::MutexGuard<'static, HashMap<String, mpsc::UnboundedSender<AndroidEvent>>> {
    ANDROID_LISTENERS.lock().unwrap_or_else(|e| {
        warn!("ANDROID_LISTENERS mutex was poisoned, recovering");
        e.into_inner()
    })
}

pub fn register_listener(job_id: &str, tx: mpsc::UnboundedSender<AndroidEvent>) {
    let mut listeners = lock_listeners();
    listeners.insert(job_id.to_string(), tx);
}

pub fn remove_listener(job_id: &str) {
    let mut listeners = lock_listeners();
    listeners.remove(job_id);
}

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();
static MAIN_ACTIVITY: OnceLock<GlobalRef> = OnceLock::new();
static YTDLP_CLASS: OnceLock<GlobalRef> = OnceLock::new();
static RUST_BRIDGE_CLASS: OnceLock<GlobalRef> = OnceLock::new();

pub fn is_jni_ready() -> bool {
    JAVA_VM.get().is_some() && MAIN_ACTIVITY.get().is_some()
}

pub async fn wait_for_jni_ready(timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    while !is_jni_ready() {
        if start.elapsed().as_millis() > timeout_ms as u128 {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    true
}

pub fn get_jni_env() -> Result<jni::AttachGuard<'static>, String> {
    let vm = JAVA_VM
        .get()
        .ok_or_else(|| "JavaVM not initialized".to_string())?;
    vm.attach_current_thread().map_err(|e| e.to_string())
}

pub fn get_activity() -> Result<&'static GlobalRef, String> {
    MAIN_ACTIVITY
        .get()
        .ok_or_else(|| "MainActivity not initialized".to_string())
}

pub fn get_ytdlp_class() -> Result<&'static GlobalRef, String> {
    YTDLP_CLASS
        .get()
        .ok_or_else(|| "YtDlp class not initialized".to_string())
}

pub fn get_rust_bridge_class() -> Result<&'static GlobalRef, String> {
    RUST_BRIDGE_CLASS
        .get()
        .ok_or_else(|| "RustBridge class not initialized".to_string())
}

pub async fn start_android_job_jni(
    job_id: &str,
    backend: &str,
    payload: &str,
    title: &str,
) -> Result<(), BackendError> {
    let job_id = job_id.to_string();
    let backend = backend.to_string();
    let payload = payload.to_string();
    let title = title.to_string();

    tokio::task::spawn_blocking(move || {
        let mut env = get_jni_env()?;
        let activity = get_activity()?;

        let j_job_id = env.new_string(&job_id).map_err(|e| e.to_string())?;
        let j_backend = env.new_string(&backend).map_err(|e| e.to_string())?;
        let j_payload = env.new_string(&payload).map_err(|e| e.to_string())?;
        let j_title = env.new_string(&title).map_err(|e| e.to_string())?;

        env.call_method(
            activity.as_obj(),
            "startJobFromRust",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Object(&j_job_id),
                JValue::Object(&j_backend),
                JValue::Object(&j_payload),
                JValue::Object(&j_title),
            ],
        )
        .map_err(|e| format!("JNI call failed: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| BackendError::Other(format!("JNI task panicked: {}", e)))?
    .map_err(BackendError::Other)
}

pub fn cancel_download_jni(job_id: &str) -> Result<(), String> {
    let mut env = get_jni_env()?;
    let activity = get_activity()?;
    let j_job_id = env.new_string(job_id).map_err(|e| e.to_string())?;
    env.call_method(
        activity.as_obj(),
        "cancelDownloadFromRust",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&j_job_id)],
    )
    .map_err(|e| format!("JNI cancel failed: {}", e))?;
    Ok(())
}

pub async fn start_resolve_jni(
    resolve_id: &str,
    url: &str,
    args_json: &str,
) -> Result<(), BackendError> {
    let resolve_id = resolve_id.to_string();
    let url = url.to_string();
    let args_json = args_json.to_string();

    tokio::task::spawn_blocking(move || {
        let mut env = get_jni_env().map_err(BackendError::Other)?;
        let cls = get_ytdlp_class().map_err(BackendError::Other)?;

        let j_id = env
            .new_string(&resolve_id)
            .map_err(|e| BackendError::Other(format!("JNI string error: {e}")))?;
        let j_url = env
            .new_string(&url)
            .map_err(|e| BackendError::Other(format!("JNI string error: {e}")))?;
        let j_args = env
            .new_string(&args_json)
            .map_err(|e| BackendError::Other(format!("JNI string error: {e}")))?;

        env.call_static_method(
            <&JClass>::from(cls.as_obj()),
            "resolveStreaming",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Object(&j_id),
                JValue::Object(&j_url),
                JValue::Object(&j_args),
            ],
        )
        .map_err(|e| BackendError::Other(format!("JNI resolveStreaming call failed: {e}")))?;
        Ok::<(), BackendError>(())
    })
    .await
    .map_err(|e| BackendError::Other(format!("JNI task panicked: {e}")))?
}

pub fn cancel_resolve_jni(resolve_id: &str) -> Result<(), String> {
    let mut env = get_jni_env()?;
    let cls = get_ytdlp_class()?;
    let j_id = env.new_string(resolve_id).map_err(|e| e.to_string())?;
    env.call_static_method(
        <&JClass>::from(cls.as_obj()),
        "cancelResolve",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&j_id)],
    )
    .map_err(|e| format!("JNI cancelResolve failed: {e}"))?;
    Ok(())
}

pub fn pause_download_jni(job_id: &str) -> Result<(), String> {
    let mut env = get_jni_env()?;
    let activity = get_activity()?;
    let j_job_id = env.new_string(job_id).map_err(|e| e.to_string())?;
    env.call_method(
        activity.as_obj(),
        "pauseDownloadFromRust",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&j_job_id)],
    )
    .map_err(|e| format!("JNI pause failed: {}", e))?;
    Ok(())
}

pub fn call_kotlin_progress_update(job_id: &str, progress: &ProgressUpdate) {
    let Ok(mut env) = get_jni_env() else { return };
    let Ok(cls) = RUST_BRIDGE_CLASS.get().ok_or("no class") else {
        return;
    };

    let percent = progress
        .total_bytes
        .map(|t| {
            if t > 0 {
                ((progress.downloaded_bytes as f64 / t as f64) * 100.0) as i32
            } else {
                0
            }
        })
        .unwrap_or(0);

    let speed_bps = progress.speed.unwrap_or(0) as i64;
    let eta_secs = progress.eta.unwrap_or(0) as i64;

    let Ok(j_job_id) = env.new_string(job_id) else {
        return;
    };

    let _ = env.call_static_method(
        <&JClass>::from(cls.as_obj()),
        "updateNotificationProgress",
        "(Ljava/lang/String;IJJ)V",
        &[
            JValue::Object(&j_job_id),
            JValue::Int(percent),
            JValue::Long(speed_bps),
            JValue::Long(eta_secs),
        ],
    );
}

pub fn update_ytdlp_channel_jni(channel: &str) -> Result<String, String> {
    let mut env = get_jni_env()?;
    let cls = get_ytdlp_class()?;
    let activity = get_activity()?;

    let app_obj = env
        .call_method(activity.as_obj(), "getApplication", "()Landroid/app/Application;", &[])
        .map_err(|e| format!("getApplication failed: {e}"))?
        .l()
        .map_err(|e| format!("getApplication return error: {e}"))?;

    let j_channel = env
        .new_string(channel)
        .map_err(|e| format!("JNI string error: {e}"))?;

    let result = env
        .call_static_method(
            <&JClass>::from(cls.as_obj()),
            "updateChannel",
            "(Landroid/app/Application;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&app_obj), JValue::Object(&j_channel)],
        )
        .map_err(|e| format!("JNI call failed: {e}"))?;

    let jstr = result.l().map_err(|e| format!("JNI return error: {e}"))?;
    if jstr.is_null() {
        return Err("updateChannel returned null".to_string());
    }
    let version: String = env
        .get_string((&jstr).into())
        .map_err(|e| format!("JNI string read error: {e}"))?
        .into();

    Ok(version)
}

pub fn call_kotlin_title_update(job_id: &str, title: &str) {
    let Ok(mut env) = get_jni_env() else { return };
    let Ok(cls) = RUST_BRIDGE_CLASS.get().ok_or("no class") else {
        return;
    };

    let Ok(j_job_id) = env.new_string(job_id) else {
        return;
    };
    let Ok(j_title) = env.new_string(title) else {
        return;
    };

    let _ = env.call_static_method(
        <&JClass>::from(cls.as_obj()),
        "updateNotificationTitle",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&j_job_id), JValue::Object(&j_title)],
    );
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_initRustJniWithActivity<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    activity: JObject<'local>,
) {
    if let Ok(vm) = env.get_java_vm() {
        let _ = JAVA_VM.set(vm);
    }
    if let Ok(cls) = env.find_class("com/nichind/comine/YtDlp") {
        if let Ok(g) = env.new_global_ref(cls) {
            let _ = YTDLP_CLASS.set(g);
        }
    }
    if let Ok(cls) = env.find_class("com/nichind/comine/RustBridge") {
        if let Ok(g) = env.new_global_ref(cls) {
            let _ = RUST_BRIDGE_CLASS.set(g);
        }
    }
    if let Ok(g) = env.new_global_ref(activity) {
        let _ = MAIN_ACTIVITY.set(g);
    }
    info!("Android JNI initialized");
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadStarted<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    _title: JString<'local>,
) {
    let job_id = jni_string!(env, job_id);
    debug!(target: "android", "Download started: {}", job_id);
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnRawOutput<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    line: JString<'local>,
) {
    let job_id = jni_string!(env, job_id);
    let line = jni_string!(env, line);

    if line.contains("__COMINE_PROGRESS__") {
        if let Some(raw_progress) = parse_raw_progress(&line, &job_id) {
            call_kotlin_progress_update(&job_id, &raw_progress);
        }
        let listeners = lock_listeners();
        if let Some(tx) = listeners.get(&job_id) {
            let _ = tx.send(AndroidEvent::RawProgressLine(line));
        }
        return;
    }

    // [#abc 5.0MiB/10.0MiB CN:8 DL:1.2MiB ETA:4s]
    if line.contains("[#") {
        let listeners = lock_listeners();
        if let Some(tx) = listeners.get(&job_id) {
            let _ = tx.send(AndroidEvent::RawProgressLine(line));
        }
        return;
    }

    if let Some(event) = parse_ytdlp_line_event(&line) {
        let listeners = lock_listeners();
        if let Some(tx) = listeners.get(&job_id) {
            match &event {
                YtdlpEvent::Title(t) => {
                    call_kotlin_title_update(&job_id, t);
                    let _ = tx.send(AndroidEvent::Metadata(event));
                }
                YtdlpEvent::Filepath(p) => {
                    let _ = tx.send(AndroidEvent::FileOutput(p.clone()));
                }
                other => {
                    let _ = tx.send(AndroidEvent::Metadata(other.clone()));
                }
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadCompleted<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    output_path: JString<'local>,
    title: JString<'local>,
    _thumbnail: JString<'local>,
) {
    let job_id = jni_string!(env, job_id);
    let output_path = jni_string!(env, output_path);
    let title: Option<String> = env
        .get_string(&title)
        .ok()
        .map(|s| String::from(s))
        .filter(|s| !s.is_empty());

    debug!(target: "android", "Download completed: {} -> {}", job_id, output_path);

    let listeners = lock_listeners();
    if let Some(tx) = listeners.get(&job_id) {
        let _ = tx.send(AndroidEvent::Completed { output_path, title });
    }
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadFailed<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    error: JString<'local>,
) {
    let job_id = jni_string!(env, job_id);
    let error = jni_string!(env, error, "Unknown error".to_string());

    warn!(target: "android", "Download failed: {} - {}", job_id, error);

    let listeners = lock_listeners();
    if let Some(tx) = listeners.get(&job_id) {
        let _ = tx.send(AndroidEvent::Failed(error));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadCancelled<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
) {
    let job_id = jni_string!(env, job_id);

    debug!(target: "android", "Download cancelled: {}", job_id);

    let listeners = lock_listeners();
    if let Some(tx) = listeners.get(&job_id) {
        let _ = tx.send(AndroidEvent::Cancelled);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadPaused<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
) {
    let job_id = jni_string!(env, job_id);

    debug!(target: "android", "Download paused: {}", job_id);

    let listeners = lock_listeners();
    if let Some(tx) = listeners.get(&job_id) {
        let _ = tx.send(AndroidEvent::Paused);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnResolveOutput<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    resolve_id: JString<'local>,
    line: JString<'local>,
) {
    let resolve_id = jni_string!(env, resolve_id);
    let line = jni_string!(env, line);

    let listeners = lock_listeners();
    if let Some(tx) = listeners.get(&resolve_id) {
        let _ = tx.send(AndroidEvent::ResolveOutput(line));
    }
}
