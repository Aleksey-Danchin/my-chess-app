use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GoMode {
    Infinite,
    #[serde(rename_all = "camelCase")]
    Depth { depth: u32 },
    #[serde(rename_all = "camelCase")]
    Movetime { ms: u64 },
    #[serde(rename_all = "camelCase")]
    Nodes { nodes: u64 },
}

impl Default for GoMode {
    fn default() -> Self {
        Self::Depth { depth: 20 }
    }
}

impl GoMode {
    pub fn to_uci(&self) -> String {
        match self {
            Self::Infinite => "go infinite".into(),
            Self::Depth { depth } => format!("go depth {depth}"),
            Self::Movetime { ms } => format!("go movetime {ms}"),
            Self::Nodes { nodes } => format!("go nodes {nodes}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSettings {
    pub go: GoMode,
    pub multipv: u32,
    pub threads: u32,
    pub hash_mb: u32,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            go: GoMode::Depth { depth: 20 },
            multipv: 3,
            threads: 4.min(super::threads::max_allowed_threads()),
            hash_mb: 128,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProfile {
    pub id: String,
    pub name: String,
    pub engine_id: String,
    pub settings: AnalysisSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisLine {
    pub multipv: u32,
    pub depth: u32,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub nodes: Option<u64>,
    pub nps: Option<u64>,
    pub time_ms: Option<u64>,
    pub pv: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisUpdate {
    pub fen: String,
    pub lines: Vec<AnalysisLine>,
    pub done: bool,
}
