use crate::blender_bridge;
use crate::db::{ensure_vault_dirs, VaultDb};
use crate::kinematics;
use crate::kinematics::motion_blender;
use crate::model_manager;
use crate::models::{
    AppSettings, BatchJob, BlenderSendOptions, BlenderStatus, ClipPreviewData, Collection,
    ExportResult, FaceTrackFile, HarvestConfig, ModelInfo, MotionClip, MuscleTrackFile,
    PolishConfig, VaultFilter,
};
use crate::pipeline::queue::{emit_queue_update, QueueController};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use walkdir::WalkDir;

pub struct AppState {
    pub db: Arc<Mutex<VaultDb>>,
    pub queue: Arc<QueueController>,
}

const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv"];

fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let db = state.db.lock().unwrap();
    db.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    ensure_vault_dirs(Path::new(&settings.vault_dir)).map_err(|e| e.to_string())?;
    let db = state.db.lock().unwrap();
    db.save_settings(&settings).map_err(|e| e.to_string())?;
    db.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let vault_dir = {
        let db = state.db.lock().unwrap();
        db.get_settings().map_err(|e| e.to_string())?.vault_dir
    };
    Ok(model_manager::check_models(Path::new(&vault_dir)))
}

#[tauri::command]
pub async fn download_models(
    app: AppHandle,
    state: State<'_, AppState>,
    package: String,
) -> Result<Vec<ModelInfo>, String> {
    let vault_dir = {
        let db = state.db.lock().unwrap();
        db.get_settings().map_err(|e| e.to_string())?.vault_dir
    };
    let vault_path = PathBuf::from(&vault_dir);
    let selected = model_manager::select_models_for_package(&vault_path, &package);

    for model in &selected {
        let app_clone = app.clone();
        model_manager::download_model(&vault_path, model, move |progress| {
            let _ = app_clone.emit("model:download_progress", progress);
        })
        .await
        .map_err(|e| format!("Gagal mengunduh {}: {}", model.name, e))?;
    }

    Ok(model_manager::check_models(&vault_path))
}

#[tauri::command]
pub fn enqueue_sources(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    ai_mode: String,
    config: HarvestConfig,
) -> Result<Vec<BatchJob>, String> {
    let mut video_files: Vec<PathBuf> = Vec::new();
    for raw_p in paths {
        let p = PathBuf::from(&raw_p);
        if p.is_file() && is_video_file(&p) {
            video_files.push(p);
        } else if p.is_dir() {
            for entry in WalkDir::new(&p).into_iter().filter_map(|e| e.ok()) {
                let ep = entry.path();
                if ep.is_file() && is_video_file(ep) {
                    video_files.push(ep.to_path_buf());
                }
            }
        }
    }

    if video_files.is_empty() {
        return Err("Tidak ada file video (.mp4/.mkv/.mov/.avi/.webm) yang ditemukan.".into());
    }

    let mut created = Vec::new();
    {
        let db = state.db.lock().unwrap();
        for vf in video_files {
            let name = vf
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("video.mp4")
                .to_string();
            let job = db
                .create_job(&vf.to_string_lossy(), &name, &ai_mode, &config)
                .map_err(|e| e.to_string())?;
            created.push(job);
        }
    }

    emit_queue_update(&state.db, &app);
    // Do NOT auto-start harvesting on upload — wait until user explicitly clicks Start!
    Ok(created)
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Vec<BatchJob>, String> {
    let db = state.db.lock().unwrap();
    db.list_jobs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.delete_job(&job_id).map_err(|e| e.to_string())?;
    }
    emit_queue_update(&state.db, &app);
    Ok(())
}

#[tauri::command]
pub fn queue_control(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    job_id: Option<String>,
) -> Result<Vec<BatchJob>, String> {
    match action.as_str() {
        "start" | "resume" => {
            state.queue.paused.store(false, Ordering::SeqCst);
            state.queue.stop_requested.store(false, Ordering::SeqCst);
            let db = state.db.lock().unwrap();
            if let Some(ref jid) = job_id {
                let _ = db.update_job_status(jid, "queued", None);
            } else {
                // Resume any paused jobs
                if let Ok(jobs) = db.list_jobs() {
                    for j in jobs {
                        if j.status == "paused" {
                            let _ = db.update_job_status(&j.id, "queued", None);
                        }
                    }
                }
            }
            drop(db);
            state.queue.wake();
        }
        "pause" => {
            state.queue.paused.store(true, Ordering::SeqCst);
            if let Some(ref jid) = job_id {
                let db = state.db.lock().unwrap();
                let _ = db.update_job_status(jid, "paused", None);
            }
        }
        "stop" | "cancel" => {
            state.queue.stop_requested.store(true, Ordering::SeqCst);
            let db = state.db.lock().unwrap();
            if let Some(ref jid) = job_id {
                let _ = db.update_job_status(jid, "cancelled", None);
            } else {
                state.queue.paused.store(true, Ordering::SeqCst);
                if let Ok(jobs) = db.list_jobs() {
                    for j in jobs {
                        if matches!(j.status.as_str(), "queued" | "splitting" | "processing") {
                            let _ = db.update_job_status(&j.id, "cancelled", None);
                        }
                    }
                }
            }
        }
        other => return Err(format!("Aksi queue tidak dikenal: {}", other)),
    }

    emit_queue_update(&state.db, &app);
    let db = state.db.lock().unwrap();
    db.list_jobs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn query_vault(
    state: State<'_, AppState>,
    filter: VaultFilter,
) -> Result<Vec<MotionClip>, String> {
    let db = state.db.lock().unwrap();
    db.query_vault(&filter).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_clip_preview(
    state: State<'_, AppState>,
    clip_id: String,
) -> Result<ClipPreviewData, String> {
    let clip = {
        let db = state.db.lock().unwrap();
        db.get_clip(&clip_id).map_err(|e| e.to_string())?
    };

    let bvh_content = clip
        .bvh_path
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok());

    let face_data = clip
        .face_path
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<FaceTrackFile>(&s).ok());

    let muscle_data = clip
        .muscle_path
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<MuscleTrackFile>(&s).ok());

    Ok(ClipPreviewData {
        clip,
        bvh_content,
        face_data,
        muscle_data,
    })
}

/// Non-destructive sub-15ms re-compilation from `*.rawperf.json` in pure Rust.
#[tauri::command]
pub fn recompile_clip(
    state: State<'_, AppState>,
    clip_id: String,
    polish_config: PolishConfig,
) -> Result<ClipPreviewData, String> {
    let (clip, vault_dir) = {
        let db = state.db.lock().unwrap();
        let c = db.get_clip(&clip_id).map_err(|e| e.to_string())?;
        let v = db.get_settings().map_err(|e| e.to_string())?.vault_dir;
        (c, PathBuf::from(v))
    };

    let raw = kinematics::load_raw_perf(Path::new(&clip.raw_perf_path))
        .map_err(|e| format!("Gagal membaca rawperf.json: {}", e))?;

    let compiled = kinematics::compile_raw_performance(&raw, &polish_config, &vault_dir, &clip_id)
        .map_err(|e| format!("Gagal mengompilasi klip: {}", e))?;

    let bvh_path_str = compiled
        .bvh_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let face_path_str = compiled
        .face_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let muscle_path_str = compiled
        .muscle_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    let updated = {
        let db = state.db.lock().unwrap();
        db.update_clip_compiled(
            &clip_id,
            compiled.frame_count as i64,
            compiled.duration_sec,
            &compiled.body_tags,
            &compiled.emotion_tags,
            bvh_path_str.as_deref(),
            face_path_str.as_deref(),
            muscle_path_str.as_deref(),
            &polish_config,
        )
        .map_err(|e| e.to_string())?
    };

    Ok(ClipPreviewData {
        clip: updated,
        bvh_content: compiled.bvh_content,
        face_data: compiled.face_data,
        muscle_data: compiled.muscle_data,
    })
}

/// Blends 2..=5 clips seamlessly using SLERP Quaternion + smoothstep facial crossfade.
#[tauri::command]
pub fn blend_clips(
    app: AppHandle,
    state: State<'_, AppState>,
    clip_ids: Vec<String>,
    transition_frames: usize,
    name: String,
) -> Result<MotionClip, String> {
    if clip_ids.len() < 2 {
        return Err("Pilih minimal 2 klip untuk digabungkan.".into());
    }

    let (source_clips, vault_dir) = {
        let db = state.db.lock().unwrap();
        let v = db.get_settings().map_err(|e| e.to_string())?.vault_dir;
        let mut list = Vec::with_capacity(clip_ids.len());
        for cid in &clip_ids {
            list.push(db.get_clip(cid).map_err(|e| e.to_string())?);
        }
        (list, PathBuf::from(v))
    };

    let mut raw_clips = Vec::with_capacity(source_clips.len());
    for sc in &source_clips {
        let raw = kinematics::load_raw_perf(Path::new(&sc.raw_perf_path))
            .map_err(|e| format!("Gagal memuat {}: {}", sc.title, e))?;
        raw_clips.push(raw);
    }

    let blended_raw = motion_blender::blend_raw_clips(&raw_clips, transition_frames)?;
    let new_id = Uuid::new_v4().to_string();
    let raw_dir = vault_dir.join("raw");
    std::fs::create_dir_all(&raw_dir).map_err(|e| e.to_string())?;
    let raw_path = raw_dir.join(format!("{}.rawperf.json", new_id));
    std::fs::write(
        &raw_path,
        serde_json::to_string(&blended_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let has_hands = source_clips.iter().any(|c| c.has_hands);
    let has_face = source_clips.iter().any(|c| c.has_face);
    let avg_quality =
        source_clips.iter().map(|c| c.quality_score).sum::<f64>() / source_clips.len() as f64;

    let polish = PolishConfig {
        include_hands: has_hands,
        include_face: has_face,
        trim_start: 0,
        trim_end: blended_raw.frames.len().saturating_sub(1),
        ..Default::default()
    };

    let compiled = kinematics::compile_raw_performance(&blended_raw, &polish, &vault_dir, &new_id)
        .map_err(|e| e.to_string())?;

    let title = if name.trim().is_empty() {
        format!(
            "Blend: {}",
            source_clips
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>()
                .join(" -> ")
        )
    } else {
        name.trim().to_string()
    };

    let clip = MotionClip {
        id: new_id,
        job_id: None,
        collection_id: None,
        clip_type: "blended".to_string(),
        person_track_id: 0,
        title,
        source_video: "Motion Blender".to_string(),
        source_start_sec: 0.0,
        source_end_sec: compiled.duration_sec,
        fps: blended_raw.fps,
        total_raw_frames: blended_raw.frames.len() as i64,
        frame_count: compiled.frame_count as i64,
        duration_sec: compiled.duration_sec,
        quality_score: avg_quality,
        has_hands,
        has_face,
        body_tags: compiled.body_tags,
        emotion_tags: compiled.emotion_tags,
        is_favorite: false,
        raw_perf_path: raw_path.to_string_lossy().to_string(),
        bvh_path: compiled.bvh_path.map(|p| p.to_string_lossy().to_string()),
        face_path: compiled.face_path.map(|p| p.to_string_lossy().to_string()),
        muscle_path: compiled.muscle_path.map(|p| p.to_string_lossy().to_string()),
        polish_config: polish,
        created_at: Utc::now().to_rfc3339(),
    };

    {
        let db = state.db.lock().unwrap();
        db.insert_clip(&clip).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("vault:clip_added", &clip);
    Ok(clip)
}

#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, clip_id: String) -> Result<MotionClip, String> {
    let db = state.db.lock().unwrap();
    db.toggle_favorite(&clip_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_clip(
    state: State<'_, AppState>,
    clip_id: String,
    title: String,
) -> Result<MotionClip, String> {
    let db = state.db.lock().unwrap();
    db.rename_clip(&clip_id, &title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_clips(state: State<'_, AppState>, clip_ids: Vec<String>) -> Result<usize, String> {
    let deleted = {
        let db = state.db.lock().unwrap();
        db.delete_clips(&clip_ids).map_err(|e| e.to_string())?
    };

    for c in &deleted {
        let _ = std::fs::remove_file(&c.raw_perf_path);
        if let Some(ref p) = c.bvh_path {
            let _ = std::fs::remove_file(p);
        }
        if let Some(ref p) = c.face_path {
            let _ = std::fs::remove_file(p);
        }
        if let Some(ref p) = c.muscle_path {
            let _ = std::fs::remove_file(p);
        }
    }

    Ok(deleted.len())
}

#[tauri::command]
pub fn manage_collections(
    state: State<'_, AppState>,
    action: String,
    name: Option<String>,
    collection_id: Option<String>,
    clip_ids: Option<Vec<String>>,
) -> Result<Vec<Collection>, String> {
    let db = state.db.lock().unwrap();
    match action.as_str() {
        "list" => {}
        "create" => {
            let n = name.unwrap_or_default();
            if n.trim().is_empty() {
                return Err("Nama koleksi tidak boleh kosong.".into());
            }
            db.create_collection(&n).map_err(|e| e.to_string())?;
        }
        "delete" => {
            if let Some(cid) = collection_id {
                db.delete_collection(&cid).map_err(|e| e.to_string())?;
            }
        }
        "assign" => {
            let ids = clip_ids.unwrap_or_default();
            db.assign_clips_to_collection(&ids, collection_id.as_deref())
                .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("Aksi koleksi tidak dikenal: {}", other)),
    }
    db.list_collections().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_clips(
    state: State<'_, AppState>,
    clip_ids: Vec<String>,
    target_dir: String,
) -> Result<ExportResult, String> {
    let target = PathBuf::from(&target_dir);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let clips = {
        let db = state.db.lock().unwrap();
        let mut out = Vec::new();
        for cid in &clip_ids {
            out.push(db.get_clip(cid).map_err(|e| e.to_string())?);
        }
        out
    };

    let mut files_written = Vec::new();
    for clip in &clips {
        let base = format!(
            "{}_{}",
            sanitize_filename(&clip.title),
            &clip.id[..clip.id.len().min(6)]
        );
        if let Some(ref p) = clip.bvh_path {
            if Path::new(p).is_file() {
                let dst = target.join(format!("{}.bvh", base));
                std::fs::copy(p, &dst).map_err(|e| e.to_string())?;
                files_written.push(dst.to_string_lossy().to_string());
            }
        }
        if let Some(ref p) = clip.face_path {
            if Path::new(p).is_file() {
                let dst = target.join(format!("{}.face.json", base));
                std::fs::copy(p, &dst).map_err(|e| e.to_string())?;
                files_written.push(dst.to_string_lossy().to_string());
            }
        }
        if let Some(ref p) = clip.muscle_path {
            if Path::new(p).is_file() {
                let dst = target.join(format!("{}.muscle.json", base));
                std::fs::copy(p, &dst).map_err(|e| e.to_string())?;
                files_written.push(dst.to_string_lossy().to_string());
            }
        }
    }

    Ok(ExportResult {
        exported_clips: clips.len(),
        files_written,
        target_dir,
    })
}

#[tauri::command]
pub async fn ping_blender(state: State<'_, AppState>) -> Result<BlenderStatus, String> {
    let port = {
        let db = state.db.lock().unwrap();
        db.get_settings().map_err(|e| e.to_string())?.blender_port
    };
    Ok(blender_bridge::ping_blender(port).await)
}

#[tauri::command]
pub async fn send_to_blender(
    state: State<'_, AppState>,
    clip_id: String,
    options: BlenderSendOptions,
) -> Result<String, String> {
    let (port, clip) = {
        let db = state.db.lock().unwrap();
        let port = db.get_settings().map_err(|e| e.to_string())?.blender_port;
        let clip = db.get_clip(&clip_id).map_err(|e| e.to_string())?;
        (port, clip)
    };
    blender_bridge::send_clip_to_blender(port, &clip, &options).await
}

/// Copies the bundled Blender 4.x Add-on script (`motion_harvest_bridge.py`)
/// to a user-selected directory so users never need to hunt for the script manually.
#[tauri::command]
pub fn copy_blender_addon(app: AppHandle, target_dir: String) -> Result<String, String> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("blender-addon").join("motion_harvest_bridge.py"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("blender-addon").join("motion_harvest_bridge.py"));
        }
    }
    if let Ok(res_dir) = app.path().resource_dir() {
        candidates.push(res_dir.join("blender-addon").join("motion_harvest_bridge.py"));
        candidates.push(res_dir.join("_up_").join("blender-addon").join("motion_harvest_bridge.py"));
        candidates.push(res_dir.join("motion_harvest_bridge.py"));
    }

    let source = candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| "File motion_harvest_bridge.py tidak ditemukan di bundle.".to_string())?;

    let dst_dir = PathBuf::from(&target_dir);
    std::fs::create_dir_all(&dst_dir).map_err(|e| e.to_string())?;
    let dst_file = dst_dir.join("motion_harvest_bridge.py");
    std::fs::copy(&source, &dst_file).map_err(|e| e.to_string())?;
    Ok(dst_file.to_string_lossy().to_string())
}
