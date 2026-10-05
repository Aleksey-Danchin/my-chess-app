use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::catalog::{fetch_catalog, slugify};
use super::DatabaseInfo;
use crate::fs::{
    databases_dir, download_to_file, ensure_dir, DownloadCanceller, DownloadProgress,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseRecord {
    pub id: String,
    pub title: String,
    pub path: String,
    pub game_count: u64,
    pub storage_size: u64,
}

fn registry_path(root: &Path) -> PathBuf {
    root.join("databases.json")
}

fn read_registry(root: &Path) -> Result<Vec<DatabaseRecord>, String> {
    let path = registry_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read_db_registry: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse_db_registry: {e}"))
}

fn write_registry(root: &Path, records: &[DatabaseRecord]) -> Result<(), String> {
    ensure_dir(root)?;
    let path = registry_path(root);
    let data =
        serde_json::to_string_pretty(records).map_err(|e| format!("serialize_db_registry: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("write_db_registry: {e}"))
}

pub fn list_installed(app: &AppHandle) -> Result<Vec<DatabaseInfo>, String> {
    let root = databases_dir(app)?;
    let records = read_registry(&root)?;
    Ok(records
        .into_iter()
        .map(|r| {
            let indexed = super::search_index::index_is_current(Path::new(&r.path));
            DatabaseInfo {
                id: r.id,
                title: r.title,
                path: r.path,
                game_count: r.game_count,
                storage_size: r.storage_size,
                installed: true,
                indexed,
            }
        })
        .collect())
}

pub fn find_installed(app: &AppHandle, db_id: &str) -> Result<DatabaseRecord, String> {
    let root = databases_dir(app)?;
    read_registry(&root)?
        .into_iter()
        .find(|r| r.id == db_id)
        .ok_or_else(|| format!("not_installed:{db_id}"))
}

pub async fn install_database(app: AppHandle, catalog_id: String) -> Result<DatabaseInfo, String> {
    let catalog = fetch_catalog()
        .await?
        .into_iter()
        .find(|d| d.id == catalog_id)
        .ok_or_else(|| format!("unknown_database:{catalog_id}"))?;

    let root = databases_dir(&app)?;
    ensure_dir(&root)?;

    let mut records = read_registry(&root)?;
    if records.iter().any(|r| r.id == catalog.id) {
        return Err(format!("already_installed:{}", catalog.id));
    }

    let file_name = format!("{}.db3", slugify(&catalog.title));
    let dest = root.join(&file_name);
    if dest.exists() {
        std::fs::remove_file(&dest).map_err(|e| format!("clean_db_file: {e}"))?;
    }

    let canceller = app.state::<DownloadCanceller>();
    let cancel = canceller.begin(&catalog.id)?;

    let progress_id = catalog.id.clone();
    let app_progress = app.clone();

    let download_result = download_to_file(
        &catalog.download_link,
        &dest,
        Some(cancel),
        |downloaded, total| {
            let _ = app_progress.emit(
                "db-download-progress",
                DownloadProgress {
                    id: progress_id.clone(),
                    downloaded,
                    total,
                },
            );
        },
    )
    .await;

    if let Err(err) = download_result {
        canceller.finish(&catalog.id);
        let _ = std::fs::remove_file(&dest);
        return Err(err);
    }

    let record = DatabaseRecord {
        id: catalog.id.clone(),
        title: catalog.title.clone(),
        path: dest.to_string_lossy().into_owned(),
        game_count: catalog.game_count,
        storage_size: catalog.storage_size,
    };
    records.push(record.clone());
    write_registry(&root, &records)?;
    canceller.finish(&catalog.id);

    Ok(DatabaseInfo {
        id: record.id,
        title: record.title,
        path: record.path,
        game_count: record.game_count,
        storage_size: record.storage_size,
        installed: true,
        indexed: false,
    })
}

/// Append an already-built database file to the local registry.
pub fn register_record(app: &AppHandle, record: DatabaseRecord) -> Result<DatabaseInfo, String> {
    let root = databases_dir(app)?;
    ensure_dir(&root)?;
    let mut records = read_registry(&root)?;
    if records.iter().any(|r| r.id == record.id) {
        return Err(format!("already_installed:{}", record.id));
    }
    let info = DatabaseInfo {
        id: record.id.clone(),
        title: record.title.clone(),
        path: record.path.clone(),
        game_count: record.game_count,
        storage_size: record.storage_size,
        installed: true,
        indexed: super::search_index::index_is_current(Path::new(&record.path)),
    };
    records.push(record);
    write_registry(&root, &records)?;
    Ok(info)
}

pub fn remove_database(app: &AppHandle, db_id: String) -> Result<(), String> {
    let root = databases_dir(app)?;
    let mut records = read_registry(&root)?;
    let removed = records
        .iter()
        .find(|r| r.id == db_id)
        .cloned()
        .ok_or_else(|| format!("not_installed:{db_id}"))?;

    records.retain(|r| r.id != db_id);
    write_registry(&root, &records)?;

    let path = PathBuf::from(&removed.path);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove_db_file: {e}"))?;
    }
    let index = super::search_index::index_path_for_db(&path);
    if index.exists() {
        let _ = std::fs::remove_file(&index);
    }
    super::search_index::clear_index_cache_for_db(&path);
    Ok(())
}
