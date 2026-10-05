use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEngine {
    pub id: String,
    pub name: String,
    pub version: String,
    pub download_url: String,
    /// Relative path of the binary inside the extracted archive (basename used as fallback).
    pub archive_path: String,
    pub download_size: u64,
}

/// Built-in catalog (Linux x86-64). Paths match encroissant.org layout where possible.
pub fn list_catalog() -> Vec<CatalogEngine> {
    #[cfg(target_os = "linux")]
    {
        vec![
            CatalogEngine {
                id: "stockfish-linux-x86-64".into(),
                name: "Stockfish".into(),
                version: "19".into(),
                download_url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-linux-x86-64-universal.tar.gz".into(),
                archive_path: "stockfish/stockfish-linux-x86-64-universal".into(),
                download_size: 81_388_977,
            },
            CatalogEngine {
                id: "rubichess-linux-avx2".into(),
                name: "RubiChess".into(),
                version: "20240817".into(),
                download_url: "https://github.com/Matthies/RubiChess/releases/download/20240817/RubiChess-20240817.zip".into(),
                archive_path: "RubiChess-20240817/linux/RubiChess-20240817_x86-64-avx2".into(),
                download_size: 31_417_660,
            },
            CatalogEngine {
                id: "dragon-linux-avx2".into(),
                name: "Dragon (Komodo)".into(),
                version: "1".into(),
                download_url: "https://komodochess.com/pub/dragon.zip".into(),
                archive_path: "dragon-linux-avx2".into(),
                download_size: 85_049_133,
            },
            CatalogEngine {
                id: "komodo-14-linux-bmi2".into(),
                name: "Komodo".into(),
                version: "14.1".into(),
                download_url: "https://komodochess.com/pub/komodo-14.zip".into(),
                archive_path: "komodo-14.1-linux-bmi2".into(),
                download_size: 9_745_847,
            },
            CatalogEngine {
                id: "stockfish-linux-arm64".into(),
                name: "Stockfish (ARM64)".into(),
                version: "19".into(),
                download_url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-linux-arm64-universal.tar.gz".into(),
                archive_path: "stockfish/stockfish-linux-arm64-universal".into(),
                download_size: 81_000_000,
            },
        ]
    }

    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

pub fn find_catalog(engine_id: &str) -> Result<CatalogEngine, String> {
    list_catalog()
        .into_iter()
        .find(|e| e.id == engine_id)
        .ok_or_else(|| format!("unknown_engine:{engine_id}"))
}
