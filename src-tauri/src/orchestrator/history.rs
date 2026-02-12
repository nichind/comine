use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use crate::orchestrator::types::HistoryItem;

const MAX_HISTORY_ITEMS: usize = 1000;

pub struct HistoryStore {
    items: RwLock<Vec<HistoryItem>>,
    path: PathBuf,
    save_notify: tokio::sync::Notify,
}

impl HistoryStore {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        let path = data_dir.join("history_backend.json");
        let items = match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str::<Vec<HistoryItem>>(&json).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        info!("History store loaded {} items from {:?}", items.len(), path);

        Arc::new(Self {
            items: RwLock::new(items),
            path,
            save_notify: tokio::sync::Notify::new(),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let store_clone = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                store_clone.save_notify.notified().await;
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                store_clone.persist().await;
            }
        });
    }

    pub async fn add(&self, item: HistoryItem) -> HistoryItem {
        let mut items = self.items.write().await;
        items.insert(0, item.clone());
        if items.len() > MAX_HISTORY_ITEMS {
            items.truncate(MAX_HISTORY_ITEMS);
        }
        self.save_notify.notify_one();
        item
    }

    pub async fn get_all(&self) -> Vec<HistoryItem> {
        self.items.read().await.clone()
    }

    pub fn get_all_blocking(&self) -> Vec<HistoryItem> {
        self.items.blocking_read().clone()
    }

    pub async fn remove(&self, id: &str) -> bool {
        let mut items = self.items.write().await;
        let before = items.len();
        items.retain(|i| i.id != id);
        let removed = items.len() < before;
        if removed {
            self.save_notify.notify_one();
        }
        removed
    }

    pub async fn clear(&self) {
        let mut items = self.items.write().await;
        items.clear();
        self.save_notify.notify_one();
    }

    pub async fn toggle_favourite(&self, id: &str) -> Option<bool> {
        let mut items = self.items.write().await;
        let item = items.iter_mut().find(|i| i.id == id)?;
        item.is_favourite = !item.is_favourite;
        let new_value = item.is_favourite;
        self.save_notify.notify_one();
        Some(new_value)
    }

    pub async fn set_favourite(&self, ids: &[String], value: bool) {
        let mut items = self.items.write().await;
        let id_set: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
        for item in items.iter_mut() {
            if id_set.contains(item.id.as_str()) {
                item.is_favourite = value;
            }
        }
        self.save_notify.notify_one();
    }

    pub async fn update_duration(&self, id: &str, duration: f64) {
        let mut items = self.items.write().await;
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.duration = duration;
            self.save_notify.notify_one();
        }
    }

    pub async fn import(&self, new_items: Vec<HistoryItem>) -> usize {
        let mut items = self.items.write().await;
        let existing_ids: std::collections::HashSet<String> =
            items.iter().map(|i| i.id.clone()).collect();
        let mut added = 0;
        for item in new_items {
            if !existing_ids.contains(&item.id) {
                items.push(item);
                added += 1;
            }
        }
        items.sort_by(|a, b| {
            b.downloaded_at
                .partial_cmp(&a.downloaded_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(MAX_HISTORY_ITEMS);
        self.save_notify.notify_one();
        added
    }

    pub async fn export(&self) -> String {
        let items = self.items.read().await;
        serde_json::to_string_pretty(&*items).unwrap_or_else(|_| "[]".to_string())
    }

    pub async fn restore_from_frontend(&self, new_items: Vec<HistoryItem>) {
        let mut items = self.items.write().await;
        if !items.is_empty() {
            let existing_ids: std::collections::HashSet<String> =
                items.iter().map(|i| i.id.clone()).collect();
            for item in new_items {
                if !existing_ids.contains(&item.id) {
                    items.push(item);
                }
            }
        } else {
            *items = new_items;
        }
        items.sort_by(|a, b| {
            b.downloaded_at
                .partial_cmp(&a.downloaded_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(MAX_HISTORY_ITEMS);
        self.save_notify.notify_one();
    }

    async fn persist(&self) {
        let items = self.items.read().await;
        let json = match serde_json::to_string_pretty(&*items) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to serialize history: {}", e);
                return;
            }
        };
        drop(items); // Release lock before I/O

        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let tmp = self.path.with_extension("json.tmp");
        match std::fs::write(&tmp, &json) {
            Ok(_) => {
                if let Err(e) = std::fs::rename(&tmp, &self.path) {
                    error!("Failed to rename history temp file: {}", e);
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            Err(e) => {
                error!("Failed to write history temp file: {}", e);
            }
        }
    }
}
