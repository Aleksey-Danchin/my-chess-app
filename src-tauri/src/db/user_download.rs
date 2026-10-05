//! Download a user's games from Lichess or Chess.com into a local `.db3`.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use super::catalog::slugify;
use super::install::{register_record, DatabaseRecord};
use super::pgn_import::convert_pgn_file;
use super::DatabaseInfo;
use crate::fs::cancel::{is_cancelled, CANCELLED};
use crate::fs::{databases_dir, ensure_dir, DownloadCanceller, DownloadProgress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserSite {
    Lichess,
    ChessCom,
}

impl UserSite {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lichess" => Ok(Self::Lichess),
            "chesscom" | "chess.com" | "chess_com" => Ok(Self::ChessCom),
            other => Err(format!("unknown_site:{other}")),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Lichess => "Lichess",
            Self::ChessCom => "Chess.com",
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            Self::Lichess => "lichess",
            Self::ChessCom => "chesscom",
        }
    }
}

fn validate_username(username: &str) -> Result<String, String> {
    let name = username.trim();
    if name.is_empty() {
        return Err("empty_username".into());
    }
    if name.len() > 40 {
        return Err("username_too_long".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("invalid_username".into());
    }
    Ok(name.to_string())
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("my-chess-app/0.1 (desktop chess GUI)")
        .build()
        .map_err(|e| format!("http_client: {e}"))
}

async fn stream_to_file(
    response: reqwest::Response,
    dest: &Path,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        ensure_dir(parent)?;
    }
    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("create_file: {e}"))?;
    let mut downloaded = 0u64;
    on_progress(downloaded, total);

    while let Some(chunk) = stream.next().await {
        if is_cancelled(cancel) {
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
    file.flush()
        .await
        .map_err(|e| format!("flush_file: {e}"))?;
    Ok(())
}

async fn download_lichess_pgn(
    client: &reqwest::Client,
    username: &str,
    dest: &Path,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
    on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let url = format!(
        "https://lichess.org/api/games/user/{username}?perfType=ultraBullet,bullet,blitz,rapid,classical,correspondence&clocks=false&evals=false&opening=false&literate=false"
    );
    let response = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/x-chess-pgn")
        .send()
        .await
        .map_err(|e| format!("lichess_request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("lichess_status: {e}"))?;
    stream_to_file(response, dest, cancel, on_progress).await
}

#[derive(Debug, Deserialize)]
struct ChessComArchives {
    archives: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ChessComGames {
    games: Vec<ChessComGame>,
}

#[derive(Debug, Deserialize)]
struct ChessComGame {
    pgn: Option<String>,
}

async fn download_chesscom_pgn(
    client: &reqwest::Client,
    username: &str,
    dest: &Path,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let archives_url = format!(
        "https://api.chess.com/pub/player/{}/games/archives",
        username.to_ascii_lowercase()
    );
    let archives: ChessComArchives = client
        .get(&archives_url)
        .send()
        .await
        .map_err(|e| format!("chesscom_archives: {e}"))?
        .error_for_status()
        .map_err(|e| format!("chesscom_archives_status: {e}"))?
        .json()
        .await
        .map_err(|e| format!("chesscom_archives_json: {e}"))?;

    if archives.archives.is_empty() {
        return Err("chesscom_no_games".into());
    }

    if let Some(parent) = dest.parent() {
        ensure_dir(parent)?;
    }
    let mut file = std::fs::File::create(dest).map_err(|e| format!("create_pgn: {e}"))?;
    let total = archives.archives.len() as u64;
    on_progress(0, Some(total));

    for (i, archive) in archives.archives.iter().enumerate() {
        if is_cancelled(cancel) {
            let _ = std::fs::remove_file(dest);
            return Err(CANCELLED.into());
        }
        let month: ChessComGames = client
            .get(archive)
            .send()
            .await
            .map_err(|e| format!("chesscom_month: {e}"))?
            .error_for_status()
            .map_err(|e| format!("chesscom_month_status: {e}"))?
            .json()
            .await
            .map_err(|e| format!("chesscom_month_json: {e}"))?;

        for game in month.games {
            if let Some(pgn) = game.pgn {
                writeln!(file, "{pgn}\n").map_err(|e| format!("write_pgn: {e}"))?;
            }
        }
        on_progress((i + 1) as u64, Some(total));
    }
    file.flush().map_err(|e| format!("flush_pgn: {e}"))?;
    Ok(())
}

pub async fn download_user_database(
    app: AppHandle,
    site: String,
    username: String,
) -> Result<DatabaseInfo, String> {
    let site = UserSite::parse(&site)?;
    let username = validate_username(&username)?;
    let db_id = format!("{}-{}", site.id_prefix(), slugify(&username));
    let title = format!("{} ({})", username, site.label());

    let root = databases_dir(&app)?;
    ensure_dir(&root)?;

    // Already installed?
    if super::install::list_installed(&app)?
        .iter()
        .any(|d| d.id == db_id)
    {
        return Err(format!("already_installed:{db_id}"));
    }

    let canceller = app.state::<DownloadCanceller>();
    let cancel = canceller.begin(&db_id)?;

    let pgn_path = root.join(format!("{db_id}.pgn"));
    let db_path = root.join(format!("{db_id}.db3"));

    let cleanup = |pgn: &Path, db: &Path| {
        let _ = std::fs::remove_file(pgn);
        let _ = std::fs::remove_file(db);
    };

    let progress_id = db_id.clone();
    let app_progress = app.clone();
    let emit_dl = |downloaded: u64, total: Option<u64>| {
        let _ = app_progress.emit(
            "db-download-progress",
            DownloadProgress {
                id: progress_id.clone(),
                downloaded,
                total,
            },
        );
    };

    let client = http_client()?;
    let download_result = match site {
        UserSite::Lichess => {
            download_lichess_pgn(&client, &username, &pgn_path, &cancel, emit_dl).await
        }
        UserSite::ChessCom => {
            download_chesscom_pgn(&client, &username, &pgn_path, &cancel, emit_dl).await
        }
    };

    if let Err(err) = download_result {
        canceller.finish(&db_id);
        cleanup(&pgn_path, &db_path);
        return Err(err);
    }

    // Convert on a blocking thread so we don't stall the async runtime.
    let convert_cancel = cancel.clone();
    let convert_pgn = pgn_path.clone();
    let convert_db = db_path.clone();
    let convert_title = title.clone();
    let convert_app = app.clone();
    let convert_id = db_id.clone();

    let convert_result = tokio::task::spawn_blocking(move || {
        convert_pgn_file(
            &convert_pgn,
            &convert_db,
            &convert_title,
            Some(convert_cancel),
            |done| {
                let _ = convert_app.emit(
                    "db-download-progress",
                    DownloadProgress {
                        id: convert_id.clone(),
                        downloaded: done,
                        total: None,
                    },
                );
            },
        )
    })
    .await
    .map_err(|e| format!("convert_join: {e}"))?;

    let _ = std::fs::remove_file(&pgn_path);

    let game_count = match convert_result {
        Ok(n) => n,
        Err(err) => {
            canceller.finish(&db_id);
            cleanup(&pgn_path, &db_path);
            return Err(err);
        }
    };

    if game_count == 0 {
        canceller.finish(&db_id);
        cleanup(&pgn_path, &db_path);
        return Err("no_games_imported".into());
    }

    let storage_size = std::fs::metadata(&db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let record = DatabaseRecord {
        id: db_id.clone(),
        title,
        path: db_path.to_string_lossy().into_owned(),
        game_count,
        storage_size,
    };

    let info = match register_record(&app, record) {
        Ok(info) => info,
        Err(err) => {
            canceller.finish(&db_id);
            cleanup(&pgn_path, &db_path);
            return Err(err);
        }
    };

    canceller.finish(&db_id);
    Ok(info)
}