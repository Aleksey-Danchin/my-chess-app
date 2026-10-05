use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEngine {
    pub id: String,
    pub name: String,
    pub version: String,
    pub download_url: String,
    /// Relative path of the binary inside the extracted archive.
    pub archive_path: String,
    pub download_size: u64,
}

/// Built-in catalog (Linux-first). Extend later for other platforms.
pub fn list_catalog() -> Vec<CatalogEngine> {
    #[cfg(target_os = "linux")]
    {
        vec![CatalogEngine {
            id: "stockfish-linux-x86-64".into(),
            name: "Stockfish".into(),
            version: "19".into(),
            download_url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-linux-x86-64-universal.tar.gz".into(),
            archive_path: "stockfish/stockfish-linux-x86-64-universal".into(),
            download_size: 82_000_000,
        }]
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
