#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("GST_PLUGIN_SYSTEM_PATH_1_0").is_err() {
            std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", "");
        }
        if std::env::var("GST_PLUGIN_PATH_1_0").is_err() {
            std::env::set_var("GST_PLUGIN_PATH_1_0", "");
        }
        if std::env::var("GST_PLUGIN_PATH").is_err() {
            std::env::set_var("GST_PLUGIN_PATH", "");
        }
        if std::env::var("GST_REGISTRY_DISABLE").is_err() {
            std::env::set_var("GST_REGISTRY_DISABLE", "yes");
        }
        if std::env::var("GTK_MODULES").is_err() {
            std::env::set_var("GTK_MODULES", "");
        }
        if std::env::var("GTK3_MODULES").is_err() {
            std::env::set_var("GTK3_MODULES", "");
        }
        if std::env::var("GDK_BACKEND").is_err() {
            std::env::set_var("GDK_BACKEND", "x11");
        }
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    comine_lib::run()
}
