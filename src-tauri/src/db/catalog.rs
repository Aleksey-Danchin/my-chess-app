use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDatabase {
    pub id: String,
    pub title: String,
    pub game_count: u64,
    pub storage_size: u64,
    pub download_link: String,
}

#[derive(Debug, Deserialize)]
struct RemoteDatabase {
    title: String,
    game_count: u64,
    storage_size: u64,
    #[serde(rename = "downloadLink")]
    download_link: String,
}

pub async fn fetch_catalog() -> Result<Vec<CatalogDatabase>, String> {
    let client = reqwest::Client::builder()
        .user_agent("my-chess-app/0.1")
        .build()
        .map_err(|e| format!("http_client: {e}"))?;

    let remote: Vec<RemoteDatabase> = client
        .get("https://www.encroissant.org/databases")
        .send()
        .await
        .map_err(|e| format!("catalog_request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("catalog_status: {e}"))?
        .json()
        .await
        .map_err(|e| format!("catalog_json: {e}"))?;

    Ok(remote
        .into_iter()
        .map(|r| CatalogDatabase {
            id: slugify(&r.title),
            title: r.title,
            game_count: r.game_count,
            storage_size: r.storage_size,
            download_link: r.download_link,
        })
        .collect())
}

pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "database".into()
    } else {
        out
    }
}
