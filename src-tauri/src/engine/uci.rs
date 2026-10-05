use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::install::list_installed;
use super::threads::clamp_threads;
use super::types::{AnalysisLine, AnalysisSettings, AnalysisUpdate, GoMode};

struct UciProcess {
    child: Child,
    stdin: ChildStdin,
    stop_reader: Arc<AtomicBool>,
}

struct ActiveAnalysis {
    process: UciProcess,
}

pub struct AnalysisManager {
    inner: Mutex<Option<ActiveAnalysis>>,
}

impl Default for AnalysisManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl AnalysisManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "analysis_lock_poisoned".to_string())?;
        if let Some(mut active) = guard.take() {
            let _ = writeln!(active.process.stdin, "stop");
            let _ = writeln!(active.process.stdin, "quit");
            active.process.stop_reader.store(true, Ordering::SeqCst);
            let _ = active.process.child.kill();
            let _ = active.process.child.wait();
        }
        Ok(())
    }

    pub fn start(
        &self,
        app: AppHandle,
        engine_id: String,
        fen: String,
        settings: AnalysisSettings,
    ) -> Result<(), String> {
        self.stop()?;

        let engines = list_installed(&app)?;
        let engine = engines
            .into_iter()
            .find(|e| e.id == engine_id)
            .ok_or_else(|| format!("engine_not_installed:{engine_id}"))?;

        let path = PathBuf::from(&engine.path);
        if !path.is_file() {
            return Err(format!("engine_binary_missing:{}", engine.path));
        }

        let mut child = Command::new(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .current_dir(path.parent().unwrap_or_else(|| std::path::Path::new(".")))
            .spawn()
            .map_err(|e| format!("spawn_engine: {e}"))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "engine_stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "engine_stdout".to_string())?;

        let stop_reader = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop_reader);
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        write_cmd(&mut stdin, "uci")?;
        wait_for(&rx, |l| l == "uciok", Duration::from_secs(5))?;
        write_cmd(
            &mut stdin,
            &format!("setoption name MultiPV value {}", settings.multipv),
        )?;
        let threads = clamp_threads(settings.threads);
        write_cmd(
            &mut stdin,
            &format!("setoption name Threads value {threads}"),
        )?;
        write_cmd(
            &mut stdin,
            &format!("setoption name Hash value {}", settings.hash_mb),
        )?;
        write_cmd(&mut stdin, "isready")?;
        wait_for(&rx, |l| l == "readyok", Duration::from_secs(5))?;
        write_cmd(&mut stdin, &format!("position fen {fen}"))?;
        write_cmd(&mut stdin, &settings.go.to_uci())?;

        let go_is_finite = !matches!(settings.go, GoMode::Infinite);
        let stop_flag = Arc::clone(&stop_reader);
        let app_reader = app.clone();
        let fen_for_thread = fen;
        let lines_shared: Arc<Mutex<HashMap<u32, AnalysisLine>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let lines_for_thread = Arc::clone(&lines_shared);

        thread::spawn(move || {
            let _keep = lines_shared;
            while !stop_flag.load(Ordering::SeqCst) {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(line) => {
                        if let Some(parsed) = parse_info_line(&line) {
                            if let Ok(mut map) = lines_for_thread.lock() {
                                map.insert(parsed.multipv, parsed);
                                emit_update(&app_reader, &fen_for_thread, &map, false);
                            }
                        } else if line.starts_with("bestmove") {
                            if let Ok(map) = lines_for_thread.lock() {
                                emit_update(&app_reader, &fen_for_thread, &map, true);
                            }
                            if go_is_finite {
                                break;
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "analysis_lock_poisoned".to_string())?;
        *guard = Some(ActiveAnalysis {
            process: UciProcess {
                child,
                stdin,
                stop_reader,
            },
        });
        Ok(())
    }
}

fn emit_update(
    app: &AppHandle,
    fen: &str,
    map: &HashMap<u32, AnalysisLine>,
    done: bool,
) {
    let mut lines: Vec<_> = map.values().cloned().collect();
    lines.sort_by_key(|l| l.multipv);
    let _ = app.emit(
        "analysis-update",
        AnalysisUpdate {
            fen: fen.to_string(),
            lines,
            done,
        },
    );
}

fn write_cmd(stdin: &mut ChildStdin, cmd: &str) -> Result<(), String> {
    writeln!(stdin, "{cmd}").map_err(|e| format!("uci_write: {e}"))?;
    stdin.flush().map_err(|e| format!("uci_flush: {e}"))
}

fn wait_for(
    rx: &std::sync::mpsc::Receiver<String>,
    pred: impl Fn(&str) -> bool,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(line) => {
                if pred(line.trim()) {
                    return Ok(());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("engine_stdout_closed".into());
            }
        }
    }
    Err("uci_handshake_timeout".into())
}

fn parse_info_line(line: &str) -> Option<AnalysisLine> {
    if !line.starts_with("info ") || !line.contains(" pv ") {
        return None;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut multipv = 1u32;
    let mut depth = 0u32;
    let mut score_cp = None;
    let mut mate = None;
    let mut nodes = None;
    let mut nps = None;
    let mut time_ms = None;
    let mut pv = Vec::new();

    let mut i = 1;
    while i < tokens.len() {
        match tokens[i] {
            "multipv" if i + 1 < tokens.len() => {
                multipv = tokens[i + 1].parse().unwrap_or(1);
                i += 2;
            }
            "depth" if i + 1 < tokens.len() => {
                depth = tokens[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            "score" if i + 2 < tokens.len() => {
                match tokens[i + 1] {
                    "cp" => score_cp = tokens[i + 2].parse().ok(),
                    "mate" => mate = tokens[i + 2].parse().ok(),
                    _ => {}
                }
                i += 3;
            }
            "nodes" if i + 1 < tokens.len() => {
                nodes = tokens[i + 1].parse().ok();
                i += 2;
            }
            "nps" if i + 1 < tokens.len() => {
                nps = tokens[i + 1].parse().ok();
                i += 2;
            }
            "time" if i + 1 < tokens.len() => {
                time_ms = tokens[i + 1].parse().ok();
                i += 2;
            }
            "pv" => {
                pv = tokens[i + 1..].iter().map(|s| (*s).to_string()).collect();
                break;
            }
            _ => i += 1,
        }
    }

    if pv.is_empty() {
        return None;
    }

    Some(AnalysisLine {
        multipv,
        depth,
        score_cp,
        mate,
        nodes,
        nps,
        time_ms,
        pv,
    })
}

/// One-shot UCI: bestmove + collected MultiPV lines for a FEN.
#[derive(Debug, Clone)]
pub struct BestMoveResult {
    pub uci: String,
    pub lines: Vec<AnalysisLine>,
}

/// One-shot UCI search. `on_lines` is called as info arrives (for live UI).
/// Pass `cancel` to abort early (sends `stop` to the engine, returns `"cancelled"`).
pub fn get_best_move(
    app: &AppHandle,
    engine_id: &str,
    fen: &str,
    settings: &AnalysisSettings,
    cancel: Option<&AtomicBool>,
    mut on_lines: impl FnMut(&[AnalysisLine]),
) -> Result<BestMoveResult, String> {
    let _ = app; // reserved for future shared events
    let engines = list_installed(app)?;
    let engine = engines
        .into_iter()
        .find(|e| e.id == engine_id)
        .ok_or_else(|| format!("engine_not_installed:{engine_id}"))?;

    let path = PathBuf::from(&engine.path);
    if !path.is_file() {
        return Err(format!("engine_binary_missing:{}", engine.path));
    }

    let mut child = Command::new(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir(path.parent().unwrap_or_else(|| std::path::Path::new(".")))
        .spawn()
        .map_err(|e| format!("spawn_engine: {e}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "engine_stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "engine_stdout".to_string())?;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let cleanup = |stdin: &mut ChildStdin, child: &mut Child| {
        let _ = writeln!(stdin, "quit");
        let _ = child.kill();
        let _ = child.wait();
    };

    let mut go = settings.go.clone();
    if matches!(go, GoMode::Infinite) {
        go = GoMode::Depth { depth: 18 };
    }

    let multipv = settings.multipv.clamp(1, 20);
    let threads = clamp_threads(settings.threads);

    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        cleanup(&mut stdin, &mut child);
        return Err("cancelled".into());
    }

    write_cmd(&mut stdin, "uci")?;
    wait_for(&rx, |l| l == "uciok", Duration::from_secs(5))?;
    write_cmd(&mut stdin, &format!("setoption name MultiPV value {multipv}"))?;
    write_cmd(&mut stdin, &format!("setoption name Threads value {threads}"))?;
    write_cmd(
        &mut stdin,
        &format!("setoption name Hash value {}", settings.hash_mb),
    )?;
    write_cmd(&mut stdin, "isready")?;
    wait_for(&rx, |l| l == "readyok", Duration::from_secs(5))?;
    write_cmd(&mut stdin, &format!("position fen {fen}"))?;
    write_cmd(&mut stdin, &go.to_uci())?;

    let timeout = match go {
        GoMode::Movetime { ms } => Duration::from_millis(ms + 5_000),
        GoMode::Depth { depth } => Duration::from_secs(30 + depth as u64),
        GoMode::Nodes { .. } => Duration::from_secs(120),
        GoMode::Infinite => Duration::from_secs(60),
    };

    let deadline = std::time::Instant::now() + timeout;
    let mut best = None;
    let mut lines_map: HashMap<u32, AnalysisLine> = HashMap::new();
    while std::time::Instant::now() < deadline {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            let _ = writeln!(stdin, "stop");
            cleanup(&mut stdin, &mut child);
            return Err("cancelled".into());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(line) => {
                if let Some(parsed) = parse_info_line(&line) {
                    lines_map.insert(parsed.multipv, parsed);
                    let mut lines: Vec<_> = lines_map.values().cloned().collect();
                    lines.sort_by_key(|l| l.multipv);
                    on_lines(&lines);
                } else if let Some(rest) = line.strip_prefix("bestmove ") {
                    best = Some(
                        rest.split_whitespace()
                            .next()
                            .unwrap_or("")
                            .to_string(),
                    );
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    cleanup(&mut stdin, &mut child);

    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err("cancelled".into());
    }

    let mv = best.ok_or_else(|| "bestmove_timeout".to_string())?;
    if mv.is_empty() || mv == "(none)" {
        return Err("no_legal_move".into());
    }
    let mut lines: Vec<_> = lines_map.values().cloned().collect();
    lines.sort_by_key(|l| l.multipv);
    on_lines(&lines);
    Ok(BestMoveResult { uci: mv, lines })
}
