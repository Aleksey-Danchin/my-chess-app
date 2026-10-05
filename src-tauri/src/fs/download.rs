use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use super::cancel::{is_cancelled, CANCELLED};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Stream `url` into `dest`, calling `on_progress` after each chunk.
/// If `cancel` is set mid-stream, deletes `dest` and returns `Err("cancelled")`.
pub async fn download_to_file<F>(
    url: &str,
    dest: &Path,
    cancel: Option<Arc<AtomicBool>>,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = dest.parent() {
        crate::fs::ensure_dir(parent)?;
    }

    if cancel.as_ref().is_some_and(|f| is_cancelled(f)) {
        return Err(CANCELLED.into());
    }

    let client = reqwest::Client::builder()
        .user_agent("my-chess-app/0.1")
        .build()
        .map_err(|e| format!("http_client: {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download_request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download_status: {e}"))?;

    if cancel.as_ref().is_some_and(|f| is_cancelled(f)) {
        return Err(CANCELLED.into());
    }

    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("create_file {}: {e}", dest.display()))?;

    let mut downloaded: u64 = 0;
    on_progress(downloaded, total);

    while let Some(chunk) = stream.next().await {
        if cancel.as_ref().is_some_and(|f| is_cancelled(f)) {
            drop(file);
            let _ = tokio::fs::remove_file(dest).await;
            return Err(CANCELLED.into());
        }

        let chunk = chunk.map_err(|e| format!("download_chunk: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write_file: {e}"))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    if cancel.as_ref().is_some_and(|f| is_cancelled(f)) {
        drop(file);
        let _ = tokio::fs::remove_file(dest).await;
        return Err(CANCELLED.into());
    }

    file.flush()
        .await
        .map_err(|e| format!("flush_file: {e}"))?;
    Ok(())
}
