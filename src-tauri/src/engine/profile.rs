use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use uuid::Uuid;

use super::install::list_installed;
use super::types::{AnalysisProfile, AnalysisSettings, GoMode};
use crate::fs::{engines_dir, ensure_dir};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfilesFile {
    profiles: Vec<AnalysisProfile>,
}

fn profiles_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(engines_dir(app)?.join("profiles.json"))
}

fn read_profiles(app: &AppHandle) -> Result<Vec<AnalysisProfile>, String> {
    let path = profiles_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read_profiles: {e}"))?;
    let file: ProfilesFile =
        serde_json::from_str(&data).map_err(|e| format!("parse_profiles: {e}"))?;
    Ok(file.profiles)
}

fn write_profiles(app: &AppHandle, profiles: &[AnalysisProfile]) -> Result<(), String> {
    let root = engines_dir(app)?;
    ensure_dir(&root)?;
    let path = profiles_path(app)?;
    let file = ProfilesFile {
        profiles: profiles.to_vec(),
    };
    let data = serde_json::to_string_pretty(&file).map_err(|e| format!("serialize_profiles: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("write_profiles: {e}"))
}

fn ensure_engine_exists(app: &AppHandle, engine_id: &str) -> Result<(), String> {
    let installed = list_installed(app)?;
    if installed.iter().any(|e| e.id == engine_id) {
        Ok(())
    } else {
        Err(format!("engine_not_installed:{engine_id}"))
    }
}

pub fn list_profiles(app: &AppHandle) -> Result<Vec<AnalysisProfile>, String> {
    read_profiles(app)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProfileInput {
    pub name: String,
    pub engine_id: String,
    pub settings: AnalysisSettings,
}

pub fn create_profile(app: &AppHandle, mut input: CreateProfileInput) -> Result<AnalysisProfile, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("empty_profile_name".into());
    }
    ensure_engine_exists(app, &input.engine_id)?;
    input.settings.threads = super::threads::clamp_threads(input.settings.threads);
    validate_settings(&input.settings)?;

    let mut profiles = read_profiles(app)?;
    let profile = AnalysisProfile {
        id: Uuid::new_v4().to_string(),
        name,
        engine_id: input.engine_id,
        settings: input.settings,
    };
    profiles.push(profile.clone());
    write_profiles(app, &profiles)?;
    Ok(profile)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileInput {
    pub id: String,
    pub name: String,
    pub engine_id: String,
    pub settings: AnalysisSettings,
}

pub fn update_profile(app: &AppHandle, mut input: UpdateProfileInput) -> Result<AnalysisProfile, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("empty_profile_name".into());
    }
    ensure_engine_exists(app, &input.engine_id)?;
    input.settings.threads = super::threads::clamp_threads(input.settings.threads);
    validate_settings(&input.settings)?;

    let mut profiles = read_profiles(app)?;
    let Some(slot) = profiles.iter_mut().find(|p| p.id == input.id) else {
        return Err(format!("profile_not_found:{}", input.id));
    };
    slot.name = name;
    slot.engine_id = input.engine_id;
    slot.settings = input.settings;
    let updated = slot.clone();
    write_profiles(app, &profiles)?;
    Ok(updated)
}

pub fn delete_profile(app: &AppHandle, profile_id: String) -> Result<(), String> {
    let mut profiles = read_profiles(app)?;
    let before = profiles.len();
    profiles.retain(|p| p.id != profile_id);
    if profiles.len() == before {
        return Err(format!("profile_not_found:{profile_id}"));
    }
    write_profiles(app, &profiles)?;
    Ok(())
}

fn validate_settings(settings: &AnalysisSettings) -> Result<(), String> {
    if settings.multipv == 0 || settings.multipv > 20 {
        return Err("invalid_multipv".into());
    }
    let max_threads = super::threads::max_allowed_threads();
    if settings.threads == 0 || settings.threads > max_threads {
        return Err(format!("invalid_threads:max={max_threads}"));
    }
    if settings.hash_mb == 0 || settings.hash_mb > 65536 {
        return Err("invalid_hash".into());
    }
    match settings.go {
        GoMode::Depth { depth } if depth == 0 || depth > 99 => Err("invalid_depth".into()),
        GoMode::Movetime { ms } if ms == 0 => Err("invalid_movetime".into()),
        GoMode::Nodes { nodes } if nodes == 0 => Err("invalid_nodes".into()),
        _ => Ok(()),
    }
}
