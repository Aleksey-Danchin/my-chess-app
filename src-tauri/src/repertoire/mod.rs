use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use shakmaty::{fen::Fen, san::SanPlus, Color, EnPassantMode, Position};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::db::{
    chess_from_fen, fen_key, move_to_uci, play_uci_on_fen, search_position_with_progress,
    PositionSearchResult,
};
use crate::engine::{clamp_threads, get_best_move, list_profiles, AnalysisLine};
use crate::fs::{ensure_dir, repertoires_dir};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireFilters {
    pub min_games: u64,
    pub min_percent: f64,
}

impl Default for RepertoireFilters {
    fn default() -> Self {
        Self {
            min_games: 5,
            min_percent: 2.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireNode {
    pub fen: String,
    pub depth_from_start: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub uci: String,
    pub san: String,
    pub source: String,
    pub games: Option<u64>,
    pub percent: Option<f64>,
    pub white: Option<u64>,
    pub draw: Option<u64>,
    pub black: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repertoire {
    pub id: String,
    pub name: String,
    pub our_color: String,
    pub setup_uci: Vec<String>,
    pub start_fen: String,
    pub db_id: String,
    pub profile_id: String,
    pub filters: RepertoireFilters,
    pub max_depth: u32,
    pub nodes: HashMap<String, RepertoireNode>,
    pub edges: Vec<RepertoireEdge>,
    /// Position keys already expanded by the builder (db or engine step done).
    #[serde(default)]
    pub expanded: HashSet<String>,
    pub status: String,
    pub editor_fen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireSummary {
    pub id: String,
    pub name: String,
    pub our_color: String,
    pub status: String,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildGraphNode {
    pub key: String,
    pub depth: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildGraphEdge {
    pub from: String,
    pub to: String,
    pub san: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCandidate {
    pub uci: String,
    pub san: String,
    pub games: Option<u64>,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    pub repertoire_id: String,
    pub nodes: usize,
    pub edges: usize,
    pub queue: usize,
    pub current_fen: String,
    pub status: String,
    /// "engine" | "db" | "" 
    pub phase: String,
    pub last_uci: Option<String>,
    pub last_san: Option<String>,
    pub engine_lines: Vec<AnalysisLine>,
    pub candidates: Vec<BuildCandidate>,
    pub graph_nodes: Vec<BuildGraphNode>,
    pub graph_edges: Vec<BuildGraphEdge>,
    /// How often the current leaf appears in the DB (games hits).
    pub leaf_games: Option<u64>,
    /// DB index scan progress (games processed / total in index).
    pub scan_done: Option<u64>,
    pub scan_total: Option<u64>,
}

#[derive(Default)]
pub struct BuildCanceller {
    flag: Mutex<Option<Arc<AtomicBool>>>,
}

impl BuildCanceller {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(&self) -> Result<(), String> {
        let mut g = self.flag.lock().map_err(|_| "lock".to_string())?;
        if g.is_some() {
            return Err("build_already_running".into());
        }
        *g = Some(Arc::new(AtomicBool::new(false)));
        Ok(())
    }

    pub fn request_cancel(&self) -> Result<(), String> {
        let g = self.flag.lock().map_err(|_| "lock".to_string())?;
        if let Some(f) = g.as_ref() {
            f.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Shared flag for parallel DB scan workers.
    pub fn cancel_flag(&self) -> Option<Arc<AtomicBool>> {
        self.flag
            .lock()
            .ok()
            .and_then(|g| g.as_ref().cloned())
    }

    pub fn is_active(&self) -> bool {
        self.flag
            .lock()
            .ok()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag()
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub fn finish(&self) {
        if let Ok(mut g) = self.flag.lock() {
            *g = None;
        }
    }
}

fn index_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(repertoires_dir(app)?.join("repertoires.json"))
}

fn repertoire_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(repertoires_dir(app)?.join(format!("{id}.json")))
}

fn read_index(app: &AppHandle) -> Result<Vec<String>, String> {
    let path = index_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read_index: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse_index: {e}"))
}

fn write_index(app: &AppHandle, ids: &[String]) -> Result<(), String> {
    let dir = repertoires_dir(app)?;
    ensure_dir(&dir)?;
    let path = index_path(app)?;
    let data = serde_json::to_string_pretty(ids).map_err(|e| format!("ser_index: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("write_index: {e}"))
}

pub fn load_repertoire(app: &AppHandle, id: &str) -> Result<Repertoire, String> {
    let path = repertoire_path(app, id)?;
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read_rep: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse_rep: {e}"))
}

pub fn save_repertoire(app: &AppHandle, rep: &Repertoire) -> Result<(), String> {
    let dir = repertoires_dir(app)?;
    ensure_dir(&dir)?;
    let path = repertoire_path(app, &rep.id)?;
    let data = serde_json::to_string_pretty(rep).map_err(|e| format!("ser_rep: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("write_rep: {e}"))?;
    let mut ids = read_index(app)?;
    if !ids.iter().any(|x| x == &rep.id) {
        ids.push(rep.id.clone());
        write_index(app, &ids)?;
    }
    Ok(())
}

pub fn list_repertoires(
    app: &AppHandle,
    canceller: &BuildCanceller,
) -> Result<Vec<RepertoireSummary>, String> {
    let ids = read_index(app)?;
    let mut out = Vec::new();
    let build_active = canceller.is_active();
    for id in ids {
        if let Ok(mut r) = load_repertoire(app, &id) {
            // Orphaned "running" after crash / failed build without cleanup.
            if r.status == "running" && !build_active {
                r.status = "idle".into();
                let _ = save_repertoire(app, &r);
            }
            out.push(RepertoireSummary {
                id: r.id,
                name: r.name,
                our_color: r.our_color,
                status: r.status,
                node_count: r.nodes.len(),
                edge_count: r.edges.len(),
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepertoireInput {
    pub name: String,
    pub our_color: String,
}

pub fn create_repertoire(
    app: &AppHandle,
    input: CreateRepertoireInput,
) -> Result<Repertoire, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("empty_name".into());
    }
    let color = match input.our_color.as_str() {
        "black" => "black",
        _ => "white",
    }
    .to_string();
    let start = START_FEN.to_string();
    let key = fen_key(&chess_from_fen(&start)?);
    let mut nodes = HashMap::new();
    nodes.insert(
        key,
        RepertoireNode {
            fen: start.clone(),
            depth_from_start: 0,
        },
    );
    let rep = Repertoire {
        id: Uuid::new_v4().to_string(),
        name,
        our_color: color,
        setup_uci: Vec::new(),
        start_fen: start.clone(),
        db_id: String::new(),
        profile_id: String::new(),
        filters: RepertoireFilters::default(),
        max_depth: 12,
        nodes,
        edges: Vec::new(),
        expanded: HashSet::new(),
        status: "idle".into(),
        editor_fen: start,
    };
    save_repertoire(app, &rep)?;
    Ok(rep)
}

pub fn delete_repertoire(app: &AppHandle, id: String) -> Result<(), String> {
    let path = repertoire_path(app, &id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove_rep: {e}"))?;
    }
    let ids: Vec<_> = read_index(app)?.into_iter().filter(|x| x != &id).collect();
    write_index(app, &ids)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRepertoireMeta {
    pub id: String,
    pub name: Option<String>,
    pub our_color: Option<String>,
    pub db_id: Option<String>,
    pub profile_id: Option<String>,
    pub filters: Option<RepertoireFilters>,
    pub max_depth: Option<u32>,
    pub start_fen: Option<String>,
    pub setup_uci: Option<Vec<String>>,
    pub editor_fen: Option<String>,
}

pub fn update_repertoire_meta(
    app: &AppHandle,
    input: UpdateRepertoireMeta,
) -> Result<Repertoire, String> {
    let mut rep = load_repertoire(app, &input.id)?;
    if let Some(n) = input.name {
        let n = n.trim().to_string();
        if !n.is_empty() {
            rep.name = n;
        }
    }
    if let Some(c) = input.our_color {
        if c == "white" || c == "black" {
            rep.our_color = c;
        }
    }
    if let Some(db) = input.db_id {
        rep.db_id = db;
    }
    if let Some(p) = input.profile_id {
        rep.profile_id = p;
    }
    if let Some(f) = input.filters {
        rep.filters = f;
    }
    if let Some(d) = input.max_depth {
        rep.max_depth = d;
    }
    if let Some(s) = input.setup_uci {
        rep.setup_uci = s;
    }
    if let Some(ef) = input.editor_fen {
        rep.editor_fen = ef;
    }
    if let Some(sf) = input.start_fen {
        let chess = chess_from_fen(&sf)?;
        let key = fen_key(&chess);
        rep.start_fen = sf.clone();
        rep.editor_fen = sf.clone();
        rep.nodes.clear();
        rep.edges.clear();
        rep.expanded.clear();
        rep.nodes.insert(
            key,
            RepertoireNode {
                fen: sf,
                depth_from_start: 0,
            },
        );
        rep.status = "idle".into();
    }
    save_repertoire(app, &rep)?;
    Ok(rep)
}

fn side_to_move_is_ours(fen: &str, our_color: &str) -> Result<bool, String> {
    let chess = chess_from_fen(fen)?;
    let ours = match our_color {
        "black" => Color::Black,
        _ => Color::White,
    };
    Ok(chess.turn() == ours)
}

fn add_edge(rep: &mut Repertoire, edge: RepertoireEdge) {
    if rep
        .edges
        .iter()
        .any(|e| e.from == edge.from && e.uci == edge.uci)
    {
        return;
    }
    rep.edges.push(edge);
}

fn find_move_for_uci(chess: &shakmaty::Chess, uci: &str) -> Option<shakmaty::Move> {
    chess
        .legal_moves()
        .into_iter()
        .find(|m| move_to_uci(m) == uci)
}

fn graph_snapshot(rep: &Repertoire) -> (Vec<BuildGraphNode>, Vec<BuildGraphEdge>) {
    let nodes = rep
        .nodes
        .iter()
        .map(|(k, n)| BuildGraphNode {
            key: k.clone(),
            depth: n.depth_from_start,
        })
        .collect();
    let edges = rep
        .edges
        .iter()
        .map(|e| BuildGraphEdge {
            from: e.from.clone(),
            to: e.to.clone(),
            san: e.san.clone(),
            source: e.source.clone(),
        })
        .collect();
    (nodes, edges)
}

fn emit_progress(
    app: &AppHandle,
    rep: &Repertoire,
    queue: usize,
    current_fen: &str,
    phase: &str,
    last_uci: Option<&str>,
    last_san: Option<&str>,
    engine_lines: &[AnalysisLine],
    candidates: &[BuildCandidate],
    leaf_games: Option<u64>,
) {
    emit_progress_scan(
        app,
        rep,
        queue,
        current_fen,
        phase,
        last_uci,
        last_san,
        engine_lines,
        candidates,
        leaf_games,
        None,
        None,
    );
}

fn emit_progress_scan(
    app: &AppHandle,
    rep: &Repertoire,
    queue: usize,
    current_fen: &str,
    phase: &str,
    last_uci: Option<&str>,
    last_san: Option<&str>,
    engine_lines: &[AnalysisLine],
    candidates: &[BuildCandidate],
    leaf_games: Option<u64>,
    scan_done: Option<u64>,
    scan_total: Option<u64>,
) {
    let (graph_nodes, graph_edges) = graph_snapshot(rep);
    let _ = app.emit(
        "repertoire-build-progress",
        BuildProgress {
            repertoire_id: rep.id.clone(),
            nodes: rep.nodes.len(),
            edges: rep.edges.len(),
            queue,
            current_fen: current_fen.to_string(),
            status: rep.status.clone(),
            phase: phase.to_string(),
            last_uci: last_uci.map(str::to_string),
            last_san: last_san.map(str::to_string),
            engine_lines: engine_lines.to_vec(),
            candidates: candidates.to_vec(),
            graph_nodes,
            graph_edges,
            leaf_games,
            scan_done,
            scan_total,
        },
    );
}

fn emit_error(app: &AppHandle, repertoire_id: &str, message: &str) {
    let _ = app.emit(
        "repertoire-build-progress",
        BuildProgress {
            repertoire_id: repertoire_id.to_string(),
            nodes: 0,
            edges: 0,
            queue: 0,
            current_fen: String::new(),
            status: format!("error:{message}"),
            phase: String::new(),
            last_uci: None,
            last_san: None,
            engine_lines: Vec::new(),
            candidates: Vec::new(),
            graph_nodes: Vec::new(),
            graph_edges: Vec::new(),
            leaf_games: None,
            scan_done: None,
            scan_total: None,
        },
    );
}

fn cached_search(
    app: &AppHandle,
    rep: &Repertoire,
    fen: &str,
    cache: &mut HashMap<String, PositionSearchResult>,
    cancel: Option<&AtomicBool>,
    on_scan: impl Fn(u64, u64) + Sync,
) -> Result<PositionSearchResult, String> {
    let key = fen_key(&chess_from_fen(fen)?);
    if let Some(hit) = cache.get(&key) {
        return Ok(hit.clone());
    }
    let result = search_position_with_progress(app, &rep.db_id, fen, cancel, on_scan)?;
    cache.insert(key, result.clone());
    Ok(result)
}

fn finish_stopped(
    app: &AppHandle,
    rep: &mut Repertoire,
    canceller: &BuildCanceller,
    pending: usize,
) -> Result<Repertoire, String> {
    rep.status = "stopped".into();
    save_repertoire(app, rep)?;
    canceller.finish();
    emit_progress(
        app,
        rep,
        pending,
        &rep.editor_fen,
        "",
        None,
        None,
        &[],
        &[],
        None,
    );
    Ok(rep.clone())
}

/// Pending leaf: highest DB frequency first, then shallower depth.
#[derive(Clone)]
struct PendingLeaf {
    key: String,
    /// Games hitting this position in the DB (0 if not yet measured).
    freq: u64,
    ours: bool,
    /// False = placeholder priority, do not show as «частота» in UI.
    freq_known: bool,
    depth: u32,
}

fn enqueue_leaf(
    app: &AppHandle,
    rep: &Repertoire,
    pending: &mut Vec<PendingLeaf>,
    cache: &mut HashMap<String, PositionSearchResult>,
    cancel: Option<&AtomicBool>,
    key: String,
    known_freq: Option<u64>,
    // If true and frequency unknown, run a DB scan now (for ranking).
    measure_if_unknown: bool,
    on_scan: impl Fn(u64, u64) + Sync,
) -> Result<(), String> {
    if pending.iter().any(|p| p.key == key) {
        return Ok(());
    }
    let Some(node) = rep.nodes.get(&key) else {
        return Ok(());
    };
    if rep.expanded.contains(&key) {
        return Ok(());
    }
    if rep.max_depth > 0 && node.depth_from_start >= rep.max_depth {
        return Ok(());
    }
    let depth = node.depth_from_start;
    let ours = side_to_move_is_ours(&node.fen, &rep.our_color).unwrap_or(false);
    let (freq, freq_known) = if let Some(f) = known_freq {
        (f, true)
    } else if measure_if_unknown {
        match cached_search(app, rep, &node.fen, cache, cancel, on_scan) {
            Ok(s) => (s.total_games, true),
            Err(e) if e == "cancelled" => return Err(e),
            Err(_) => (0, true),
        }
    } else {
        (0, false)
    };
    pending.push(PendingLeaf {
        key,
        freq,
        ours,
        freq_known,
        depth,
    });
    Ok(())
}

/// Pick next leaf: max frequency, then shallower depth, then our-to-move as weak tiebreak.
fn take_next_leaf(pending: &mut Vec<PendingLeaf>) -> Option<PendingLeaf> {
    if pending.is_empty() {
        return None;
    }
    let idx = pending
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            a.freq
                .cmp(&b.freq)
                .then_with(|| b.depth.cmp(&a.depth)) // shallower first
                .then_with(|| a.ours.cmp(&b.ours)) // prefer ours if still tied
        })
        .map(|(i, _)| i)?;
    Some(pending.swap_remove(idx))
}

/// Validate + mark running, then spawn the build loop on a background thread.
/// Returns immediately so the UI stays responsive.
pub fn spawn_build(
    app: AppHandle,
    canceller: Arc<BuildCanceller>,
    repertoire_id: String,
    rebuild: bool,
) -> Result<Repertoire, String> {
    let mut rep = load_repertoire(&app, &repertoire_id)?;
    if rep.db_id.is_empty() {
        return Err("db_not_set".into());
    }
    if rep.profile_id.is_empty() {
        return Err("profile_not_set".into());
    }
    let _profile = list_profiles(&app)?
        .into_iter()
        .find(|p| p.id == rep.profile_id)
        .ok_or_else(|| "profile_not_found".to_string())?;

    canceller.begin()?;

    if rebuild {
        let sf = rep.start_fen.clone();
        let key = fen_key(&chess_from_fen(&sf)?);
        rep.nodes.clear();
        rep.edges.clear();
        rep.expanded.clear();
        rep.nodes.insert(
            key,
            RepertoireNode {
                fen: sf,
                depth_from_start: 0,
            },
        );
    }

    rep.status = "running".into();
    save_repertoire(&app, &rep)?;
    emit_progress(
        &app,
        &rep,
        rep.nodes.len(),
        &rep.start_fen,
        "",
        None,
        None,
        &[],
        &[],
        None,
    );

    let app_bg = app.clone();
    let cancel_bg = Arc::clone(&canceller);
    let id_bg = repertoire_id.clone();
    thread::spawn(move || {
        if let Err(e) = build_repertoire_loop(&app_bg, &cancel_bg, id_bg.clone(), rebuild) {
            let _ = load_repertoire(&app_bg, &id_bg).map(|mut r| {
                r.status = "idle".into();
                let _ = save_repertoire(&app_bg, &r);
            });
            emit_error(&app_bg, &id_bg, &e);
            cancel_bg.finish();
        }
    });

    Ok(rep)
}

pub fn spawn_continue(
    app: AppHandle,
    canceller: Arc<BuildCanceller>,
    repertoire_id: String,
) -> Result<Repertoire, String> {
    let mut rep = load_repertoire(&app, &repertoire_id)?;
    let keys: Vec<_> = rep.nodes.keys().cloned().collect();
    for k in keys {
        let fen = rep.nodes[&k].fen.clone();
        let ours = side_to_move_is_ours(&fen, &rep.our_color)?;
        let has = if ours {
            rep.edges
                .iter()
                .any(|e| e.from == k && e.source == "engine")
        } else {
            rep.edges.iter().any(|e| e.from == k && e.source == "db")
        };
        if !has {
            rep.expanded.remove(&k);
        }
    }
    save_repertoire(&app, &rep)?;
    drop(rep);
    spawn_build(app, canceller, repertoire_id, false)
}

fn build_repertoire_loop(
    app: &AppHandle,
    canceller: &BuildCanceller,
    repertoire_id: String,
    _rebuild: bool,
) -> Result<Repertoire, String> {
    let mut rep = load_repertoire(app, &repertoire_id)?;
    let profile = list_profiles(app)?
        .into_iter()
        .find(|p| p.id == rep.profile_id)
        .ok_or_else(|| "profile_not_found".to_string())?;
    let mut settings = profile.settings.clone();
    settings.threads = clamp_threads(settings.threads);
    let engine_id = profile.engine_id.clone();

    let mut search_cache: HashMap<String, PositionSearchResult> = HashMap::new();
    let mut pending: Vec<PendingLeaf> = Vec::new();
    let cancel_flag = canceller.cancel_flag();
    let cancel_ref = cancel_flag.as_deref();
    let keys: Vec<_> = rep.nodes.keys().cloned().collect();
    for k in keys {
        if canceller.is_cancelled() {
            return finish_stopped(app, &mut rep, canceller, pending.len());
        }
        let fen = rep.nodes[&k].fen.clone();
        let ours = side_to_move_is_ours(&fen, &rep.our_color).unwrap_or(false);
        let enqueue_result = if ours {
            enqueue_leaf(
                app,
                &rep,
                &mut pending,
                &mut search_cache,
                cancel_ref,
                k,
                Some(0),
                false,
                |_, _| {},
            )
        } else {
            let app_s = app.clone();
            let rep_s = rep.clone();
            let fen_s = fen.clone();
            enqueue_leaf(
                app,
                &rep,
                &mut pending,
                &mut search_cache,
                cancel_ref,
                k,
                None,
                true,
                move |done, total| {
                    emit_progress_scan(
                        &app_s,
                        &rep_s,
                        0,
                        &fen_s,
                        "db-scan",
                        None,
                        None,
                        &[],
                        &[],
                        None,
                        Some(done),
                        Some(total),
                    );
                },
            )
        };
        if enqueue_result.is_err() {
            return finish_stopped(app, &mut rep, canceller, pending.len());
        }
    }

    while let Some(leaf) = take_next_leaf(&mut pending) {
        if canceller.is_cancelled() {
            return finish_stopped(app, &mut rep, canceller, pending.len());
        }

        let from_key = leaf.key;
        if rep.expanded.contains(&from_key) {
            continue;
        }
        let Some(node) = rep.nodes.get(&from_key).cloned() else {
            continue;
        };
        if rep.max_depth > 0 && node.depth_from_start >= rep.max_depth {
            rep.expanded.insert(from_key);
            continue;
        }

        let leaf_games = if leaf.freq_known {
            Some(leaf.freq)
        } else {
            None
        };
        let ours = leaf.ours;

        if ours {
            emit_progress(
                app,
                &rep,
                pending.len(),
                &node.fen,
                "engine",
                None,
                None,
                &[],
                &[],
                leaf_games,
            );
            let app_progress = app.clone();
            let rep_id = rep.id.clone();
            let fen_for_cb = node.fen.clone();
            let status = rep.status.clone();
            let queue_len = pending.len();
            let (g_nodes, g_edges) = graph_snapshot(&rep);
            let nodes_n = rep.nodes.len();
            let edges_n = rep.edges.len();

            let mut last_emit = std::time::Instant::now()
                - std::time::Duration::from_millis(200);
            let cancel_engine = cancel_ref;
            match get_best_move(
                app,
                &engine_id,
                &node.fen,
                &settings,
                cancel_engine,
                |lines| {
                let now = std::time::Instant::now();
                if now.duration_since(last_emit) < std::time::Duration::from_millis(150) {
                    return;
                }
                last_emit = now;
                let _ = app_progress.emit(
                    "repertoire-build-progress",
                    BuildProgress {
                        repertoire_id: rep_id.clone(),
                        nodes: nodes_n,
                        edges: edges_n,
                        queue: queue_len,
                        current_fen: fen_for_cb.clone(),
                        status: status.clone(),
                        phase: "engine".into(),
                        last_uci: None,
                        last_san: None,
                        engine_lines: lines.to_vec(),
                        candidates: Vec::new(),
                        graph_nodes: g_nodes.clone(),
                        graph_edges: g_edges.clone(),
                        leaf_games,
                        scan_done: None,
                        scan_total: None,
                    },
                );
            },
            ) {
                Ok(best) => {
                    let uci = best.uci;
                    if let Ok((chess_to, to_key)) = play_uci_on_fen(&node.fen, &uci) {
                        let from_chess = chess_from_fen(&node.fen)?;
                        let san = find_move_for_uci(&from_chess, &uci)
                            .map(|mv| SanPlus::from_move(from_chess.clone(), mv).to_string())
                            .unwrap_or_else(|| uci.clone());
                        let to_fen =
                            Fen::from_position(&chess_to, EnPassantMode::Legal).to_string();
                        let depth = node.depth_from_start + 1;
                        let is_new = !rep.nodes.contains_key(&to_key);
                        rep.nodes.entry(to_key.clone()).or_insert(RepertoireNode {
                            fen: to_fen.clone(),
                            depth_from_start: depth,
                        });
                        add_edge(
                            &mut rep,
                            RepertoireEdge {
                                id: Uuid::new_v4().to_string(),
                                from: from_key.clone(),
                                to: to_key.clone(),
                                uci: uci.clone(),
                                san: san.clone(),
                                source: "engine".into(),
                                games: None,
                                percent: None,
                                white: None,
                                draw: None,
                                black: None,
                            },
                        );
                        if is_new && (rep.max_depth == 0 || depth < rep.max_depth) {
                            // Inherit line popularity so deep engine replies don't
                            // outrank unexpanded shallow high-frequency leaves.
                            let inherited = if leaf.freq_known {
                                Some(leaf.freq)
                            } else {
                                leaf_games
                            };
                            if enqueue_leaf(
                                app,
                                &rep,
                                &mut pending,
                                &mut search_cache,
                                cancel_ref,
                                to_key,
                                inherited,
                                false,
                                |_, _| {},
                            )
                            .is_err()
                            {
                                return finish_stopped(
                                    app,
                                    &mut rep,
                                    canceller,
                                    pending.len(),
                                );
                            }
                        }
                        // Show the position after our move (opponent to reply).
                        emit_progress(
                            app,
                            &rep,
                            pending.len(),
                            &to_fen,
                            "engine",
                            Some(&uci),
                            Some(&san),
                            &best.lines,
                            &[],
                            leaf_games,
                        );
                    }
                }
                Err(e) if e == "cancelled" => {
                    return finish_stopped(app, &mut rep, canceller, pending.len());
                }
                Err(_) => {}
            }
            rep.expanded.insert(from_key);
        } else {
            emit_progress(
                app,
                &rep,
                pending.len(),
                &node.fen,
                "db",
                None,
                None,
                &[],
                &[],
                leaf_games,
            );
            let app_s = app.clone();
            let rep_s = rep.clone();
            let fen_s = node.fen.clone();
            let qlen = pending.len();
            let lg = leaf_games;
            let stats = match cached_search(
                app,
                &rep,
                &node.fen,
                &mut search_cache,
                cancel_ref,
                move |done, total| {
                    emit_progress_scan(
                        &app_s,
                        &rep_s,
                        qlen,
                        &fen_s,
                        "db-scan",
                        None,
                        None,
                        &[],
                        &[],
                        lg,
                        Some(done),
                        Some(total),
                    );
                },
            ) {
                Ok(s) => s,
                Err(e) if e == "cancelled" => {
                    return finish_stopped(app, &mut rep, canceller, pending.len());
                }
                Err(e) => {
                    canceller.finish();
                    rep.status = "idle".into();
                    save_repertoire(app, &rep)?;
                    return Err(e);
                }
            };

            let leaf_games = Some(stats.total_games);

            let filtered: Vec<_> = stats
                .moves
                .into_iter()
                .filter(|m| {
                    m.games >= rep.filters.min_games && m.percent >= rep.filters.min_percent
                })
                .collect();

            let candidates: Vec<BuildCandidate> = filtered
                .iter()
                .map(|m| BuildCandidate {
                    uci: m.uci.clone(),
                    san: m.san.clone(),
                    games: Some(m.games),
                    percent: Some(m.percent),
                })
                .collect();
            emit_progress(
                app,
                &rep,
                pending.len(),
                &node.fen,
                "db",
                None,
                None,
                &[],
                &candidates,
                leaf_games,
            );

            for m in filtered {
                let Ok((chess_to, to_key)) = play_uci_on_fen(&node.fen, &m.uci) else {
                    continue;
                };
                let to_fen = Fen::from_position(&chess_to, EnPassantMode::Legal).to_string();
                let depth = node.depth_from_start + 1;
                let is_new = !rep.nodes.contains_key(&to_key);
                rep.nodes.entry(to_key.clone()).or_insert(RepertoireNode {
                    fen: to_fen,
                    depth_from_start: depth,
                });
                if let Some(n) = rep.nodes.get_mut(&to_key) {
                    if depth < n.depth_from_start {
                        n.depth_from_start = depth;
                    }
                }
                add_edge(
                    &mut rep,
                    RepertoireEdge {
                        id: Uuid::new_v4().to_string(),
                        from: from_key.clone(),
                        to: to_key.clone(),
                        uci: m.uci.clone(),
                        san: m.san.clone(),
                        source: "db".into(),
                        games: Some(m.games),
                        percent: Some(m.percent),
                        white: Some(m.white),
                        draw: Some(m.draw),
                        black: Some(m.black),
                    },
                );
                if (is_new || !rep.expanded.contains(&to_key))
                    && (rep.max_depth == 0 || depth < rep.max_depth)
                {
                    // Child frequency ≈ how often this reply was played from parent.
                    if enqueue_leaf(
                        app,
                        &rep,
                        &mut pending,
                        &mut search_cache,
                        cancel_ref,
                        to_key,
                        Some(m.games),
                        false,
                        |_, _| {},
                    )
                    .is_err()
                    {
                        return finish_stopped(app, &mut rep, canceller, pending.len());
                    }
                }
                emit_progress(
                    app,
                    &rep,
                    pending.len(),
                    &node.fen,
                    "db",
                    Some(&m.uci),
                    Some(&m.san),
                    &[],
                    &candidates,
                    leaf_games,
                );
            }
            rep.expanded.insert(from_key);
        }

        save_repertoire(app, &rep)?;
    }

    rep.status = "done".into();
    save_repertoire(app, &rep)?;
    canceller.finish();
    emit_progress(app, &rep, 0, &rep.start_fen, "", None, None, &[], &[], None);
    Ok(rep)
}
