use std::fs::{self, File};
use std::io::{copy, BufReader};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use serde::{Deserialize, Serialize};
use tar::Archive;
use tauri::{AppHandle, Emitter, Manager};

use super::catalog::{find_catalog, CatalogEngine};
use super::EngineInfo;
use crate::fs::{
    download_to_file, ensure_dir, engines_dir, DownloadCanceller, DownloadProgress,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineRecord {
    id: String,
    name: String,
    version: String,
    path: String,
}

fn registry_path(engines_root: &Path) -> PathBuf {
    engines_root.join("engines.json")
}

fn read_registry(engines_root: &Path) -> Result<Vec<EngineRecord>, String> {
    let path = registry_path(engines_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read_registry: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse_registry: {e}"))
}

fn write_registry(engines_root: &Path, records: &[EngineRecord]) -> Result<(), String> {
    ensure_dir(engines_root)?;
    let path = registry_path(engines_root);
    let data = serde_json::to_string_pretty(records).map_err(|e| format!("serialize_registry: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("write_registry: {e}"))
}

pub fn list_installed(app: &AppHandle) -> Result<Vec<EngineInfo>, String> {
    let root = engines_dir(app)?;
    let records = read_registry(&root)?;
    Ok(records
        .into_iter()
        .map(|r| EngineInfo {
            id: r.id,
            name: r.name,
            version: r.version,
            path: r.path,
            installed: true,
        })
        .collect())
}

pub async fn install_engine(app: AppHandle, engine_id: String) -> Result<EngineInfo, String> {
    let catalog = find_catalog(&engine_id)?;
    let root = engines_dir(&app)?;
    ensure_dir(&root)?;

    let mut records = read_registry(&root)?;
    if records.iter().any(|r| r.id == engine_id) {
        return Err(format!("already_installed:{engine_id}"));
    }

    let engine_dir = root.join(&catalog.id);
    if engine_dir.exists() {
        std::fs::remove_dir_all(&engine_dir)
            .map_err(|e| format!("clean_engine_dir: {e}"))?;
    }
    ensure_dir(&engine_dir)?;

    let canceller = app.state::<DownloadCanceller>();
    let cancel = canceller.begin(&catalog.id)?;

    let archive_name = archive_file_name(&catalog.download_url);
    let archive_path = engine_dir.join(&archive_name);
    let progress_id = catalog.id.clone();
    let app_progress = app.clone();

    let download_result = download_to_file(
        &catalog.download_url,
        &archive_path,
        Some(cancel),
        |downloaded, total| {
            let _ = app_progress.emit(
                "engine-download-progress",
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
        let _ = std::fs::remove_dir_all(&engine_dir);
        return Err(err);
    }

    let extract_result = (|| {
        extract_archive(&archive_path, &engine_dir)?;
        let _ = std::fs::remove_file(&archive_path);
        let binary = resolve_binary(&engine_dir, &catalog)?;
        set_executable(&binary)?;
        Ok::<_, String>(binary)
    })();

    match extract_result {
        Ok(binary) => {
            let record = EngineRecord {
                id: catalog.id.clone(),
                name: catalog.name.clone(),
                version: catalog.version.clone(),
                path: binary.to_string_lossy().into_owned(),
            };
            records.push(record.clone());
            write_registry(&root, &records)?;
            canceller.finish(&catalog.id);
            Ok(EngineInfo {
                id: record.id,
                name: record.name,
                version: record.version,
                path: record.path,
                installed: true,
            })
        }
        Err(err) => {
            canceller.finish(&catalog.id);
            let _ = std::fs::remove_dir_all(&engine_dir);
            Err(err)
        }
    }
}

pub fn remove_engine(app: &AppHandle, engine_id: String) -> Result<(), String> {
    let root = engines_dir(app)?;
    let mut records = read_registry(&root)?;
    let before = records.len();
    records.retain(|r| r.id != engine_id);
    if records.len() == before {
        return Err(format!("not_installed:{engine_id}"));
    }
    write_registry(&root, &records)?;

    let engine_dir = root.join(&engine_id);
    if engine_dir.exists() {
        std::fs::remove_dir_all(&engine_dir)
            .map_err(|e| format!("remove_engine_dir: {e}"))?;
    }
    Ok(())
}

fn archive_file_name(url: &str) -> String {
    url.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download.tar.gz")
        .to_string()
}

fn extract_archive(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let name = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let file = File::open(archive_path).map_err(|e| format!("open_archive: {e}"))?;
    let reader = BufReader::new(file);

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let decoder = flate2::read::GzDecoder::new(reader);
        let mut archive = Archive::new(decoder);
        archive
            .unpack(dest)
            .map_err(|e| format!("unpack_tar_gz: {e}"))?;
    } else if name.ends_with(".tar") {
        let mut archive = Archive::new(reader);
        archive
            .unpack(dest)
            .map_err(|e| format!("unpack_tar: {e}"))?;
    } else if name.ends_with(".zip") {
        extract_zip(archive_path, dest)?;
    } else {
        return Err(format!("unsupported_archive:{name}"));
    }
    Ok(())
}

fn extract_zip(archive_path: &Path, dest: &Path) -> Result<(), String> {
    ensure_dir(dest)?;
    let file = File::open(archive_path).map_err(|e| format!("open_zip: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("read_zip: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("zip_entry: {e}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| format!("zip_mkdir: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("zip_parent: {e}"))?;
            }
            let mut out_file =
                File::create(&out_path).map_err(|e| format!("zip_create: {e}"))?;
            copy(&mut entry, &mut out_file).map_err(|e| format!("zip_extract: {e}"))?;
        }
    }
    Ok(())
}

fn resolve_binary(engine_dir: &Path, catalog: &CatalogEngine) -> Result<PathBuf, String> {
    let preferred = engine_dir.join(&catalog.archive_path);
    if preferred.is_file() {
        return Ok(preferred);
    }

    // Fallback: search for file with the same basename.
    let name = Path::new(&catalog.archive_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid_archive_path".to_string())?;

    find_file_named(engine_dir, name)
        .ok_or_else(|| format!("binary_not_found:{name}"))
}

fn find_file_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_named(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}

fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(path)
            .map_err(|e| format!("chmod_meta: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(|e| format!("chmod: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
