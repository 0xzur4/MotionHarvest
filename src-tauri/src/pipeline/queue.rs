use crate::db::VaultDb;
use crate::kinematics;
use crate::models::{MotionClip, PolishConfig, SegmentClassifiedEvent};
use crate::pipeline::sidecar::{resolve_sidecar_dir, ExtractRequest, SidecarEvent, WarmSidecar};
use crate::pipeline::splitter;
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use uuid::Uuid;

pub struct QueueController {
    pub db: Arc<Mutex<VaultDb>>,
    pub paused: Arc<AtomicBool>,
    pub stop_requested: Arc<AtomicBool>,
    pub notify: Arc<Notify>,
    pub resource_dir: Option<PathBuf>,
}

impl QueueController {
    pub fn new(db: Arc<Mutex<VaultDb>>, resource_dir: Option<PathBuf>) -> Self {
        Self {
            db,
            // Start paused until the user explicitly clicks Start (never auto-run on upload/startup!)
            paused: Arc::new(AtomicBool::new(true)),
            stop_requested: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            resource_dir,
        }
    }

    pub fn wake(&self) {
        self.notify.notify_one();
    }

    /// Starts the persistent background tokio queue loop (never blocks the UI thread).
    pub fn spawn_worker(self: Arc<Self>, app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            let mut sidecar: Option<WarmSidecar> = None;

            loop {
                if self.paused.load(Ordering::SeqCst) {
                    self.notify.notified().await;
                    continue;
                }

                let next_job = {
                    let db = self.db.lock().unwrap();
                    db.next_runnable_job().ok().flatten()
                };

                let Some(job) = next_job else {
                    self.notify.notified().await;
                    continue;
                };

                self.stop_requested.store(false, Ordering::SeqCst);
                self.process_job(&app, &mut sidecar, job.id).await;
            }
        });
    }

    async fn process_job(&self, app: &AppHandle, sidecar: &mut Option<WarmSidecar>, job_id: String) {
        let (job, settings, existing_segments) = {
            let db = self.db.lock().unwrap();
            let Ok(j) = db.get_job(&job_id) else { return };
            let Ok(s) = db.get_settings() else { return };
            let segs = db.list_segments_for_job(&job_id).unwrap_or_default();
            (j, s, segs)
        };

        let vault_dir = PathBuf::from(&settings.vault_dir);

        // Stage 1: Scene-cut splitting (skip if segments already exist from prior resume!)
        if existing_segments.is_empty() {
            {
                let db = self.db.lock().unwrap();
                let _ = db.update_job_status(&job_id, "splitting", None);
            }
            emit_queue_update(&self.db, app);

            let src_path = PathBuf::from(&job.source_path);
            let temp_dir = vault_dir.join("temp");
            let cfg = job.config.clone();
            let nvenc = settings.use_nvenc.clone();
            let jid_clone = job_id.clone();

            let split_res = tokio::task::spawn_blocking(move || {
                splitter::split_video_into_segments(&jid_clone, &src_path, &temp_dir, &cfg, &nvenc)
            })
            .await;

            match split_res {
                Ok(Ok((info, segments))) => {
                    let db = self.db.lock().unwrap();
                    let _ = db.update_job_metadata(&job_id, info.duration_sec, segments.len() as i64);
                    if let Err(e) = db.insert_segments(&segments) {
                        let _ = db.update_job_status(&job_id, "failed", Some(&e.to_string()));
                        drop(db);
                        emit_queue_update(&self.db, app);
                        return;
                    }
                    let _ = db.update_job_status(&job_id, "processing", None);
                }
                Ok(Err(e)) => {
                    let db = self.db.lock().unwrap();
                    let _ = db.update_job_status(&job_id, "failed", Some(&e.to_string()));
                    drop(db);
                    emit_queue_update(&self.db, app);
                    return;
                }
                Err(e) => {
                    let db = self.db.lock().unwrap();
                    let _ = db.update_job_status(&job_id, "failed", Some(&e.to_string()));
                    drop(db);
                    emit_queue_update(&self.db, app);
                    return;
                }
            }
            emit_queue_update(&self.db, app);
        } else {
            let db = self.db.lock().unwrap();
            let _ = db.update_job_status(&job_id, "processing", None);
            drop(db);
            emit_queue_update(&self.db, app);
        }

        // Ensure warm Python sidecar is running
        let needs_spawn = sidecar.as_mut().map(|s| !s.is_alive()).unwrap_or(true);
        if needs_spawn {
            let Some(sdir) = resolve_sidecar_dir(self.resource_dir.as_deref()) else {
                let db = self.db.lock().unwrap();
                let _ = db.update_job_status(
                    &job_id,
                    "failed",
                    Some("Folder sidecar-pose/worker.py tidak ditemukan"),
                );
                drop(db);
                emit_queue_update(&self.db, app);
                return;
            };
            match WarmSidecar::spawn(&sdir, &vault_dir).await {
                Ok(w) => *sidecar = Some(w),
                Err(e) => {
                    let db = self.db.lock().unwrap();
                    let _ = db.update_job_status(
                        &job_id,
                        "failed",
                        Some(&format!("Gagal menjalankan AI sidecar: {}", e)),
                    );
                    drop(db);
                    emit_queue_update(&self.db, app);
                    return;
                }
            }
        }

        // Stage 2 + Stage 3: Process pending segments one-by-one (resumable & cancellable!)
        let mut consecutive_failures: u32 = 0;
        loop {
            if self.paused.load(Ordering::SeqCst) {
                let db = self.db.lock().unwrap();
                let _ = db.update_job_status(&job_id, "paused", None);
                drop(db);
                emit_queue_update(&self.db, app);
                return;
            }
            if self.stop_requested.load(Ordering::SeqCst) {
                let db = self.db.lock().unwrap();
                let cur = db.get_job(&job_id).ok().map(|j| j.status).unwrap_or_default();
                if cur != "cancelled" {
                    let _ = db.update_job_status(&job_id, "paused", None);
                }
                drop(db);
                emit_queue_update(&self.db, app);
                return;
            }

            let seg_opt = {
                let db = self.db.lock().unwrap();
                // Re-check job wasn't cancelled/paused by user command
                if let Ok(j) = db.get_job(&job_id) {
                    if j.status == "paused" || j.status == "cancelled" {
                        return;
                    }
                }
                db.next_pending_segment(&job_id).ok().flatten()
            };

            let Some(seg) = seg_opt else {
                let db = self.db.lock().unwrap();
                let _ = db.refresh_job_counters(&job_id);
                let _ = db.update_job_status(&job_id, "completed", None);
                drop(db);
                emit_queue_update(&self.db, app);
                return;
            };

            {
                let db = self.db.lock().unwrap();
                let _ = db.update_segment_status(&seg.id, "extracting", None, None);
            }

            // Re-cut temp segment if resuming after temp file cleanup
            let temp_video = match seg.temp_video_path.as_deref() {
                Some(p) if Path::new(p).is_file() => PathBuf::from(p),
                _ => {
                    let recut = vault_dir
                        .join("temp")
                        .join(format!("{}_seg_{:04}.mp4", job_id, seg.segment_index));
                    let src = PathBuf::from(&job.source_path);
                    let s0 = seg.start_sec;
                    let s1 = seg.end_sec;
                    let recut_c = recut.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        splitter::cut_segment_file(&src, &recut_c, s0, s1, false)
                    })
                    .await;
                    recut
                }
            };

            let req = ExtractRequest {
                cmd: "extract".to_string(),
                segment_id: seg.id.clone(),
                video_path: temp_video.to_string_lossy().to_string(),
                ai_mode: job.ai_mode.clone(),
                multi_person: job.config.multi_person,
                max_persons: job.config.max_persons,
                min_visibility: job.config.min_visibility,
                output_dir: vault_dir.join("raw").to_string_lossy().to_string(),
                models_dir: vault_dir.join("models").to_string_lossy().to_string(),
            };

            let mut detected_class: Option<String> = None;
            let mut completed_tracks = Vec::new();

            // Re-spawn warm sidecar if a previous segment error cleared it
            if sidecar.as_mut().map(|s| !s.is_alive()).unwrap_or(true) {
                let Some(sdir) = resolve_sidecar_dir(self.resource_dir.as_deref()) else {
                    let db = self.db.lock().unwrap();
                    let _ = db.update_job_status(
                        &job_id,
                        "failed",
                        Some("Folder sidecar-pose/worker.py tidak ditemukan"),
                    );
                    drop(db);
                    emit_queue_update(&self.db, app);
                    return;
                };
                match WarmSidecar::spawn(&sdir, &vault_dir).await {
                    Ok(w) => *sidecar = Some(w),
                    Err(e) => {
                        let db = self.db.lock().unwrap();
                        let _ = db.update_job_status(
                            &job_id,
                            "failed",
                            Some(&format!("Gagal menjalankan AI sidecar: {}", e)),
                        );
                        drop(db);
                        emit_queue_update(&self.db, app);
                        return;
                    }
                }
            }

            let worker = sidecar.as_mut().unwrap();
            let extract_res = worker
                .extract_segment(&req, |evt| match evt {
                    SidecarEvent::Classified {
                        segment_id,
                        detected_class: cls,
                        person_count,
                    } => {
                        detected_class = Some(cls.clone());
                        let _ = app.emit(
                            "segment:classified",
                            SegmentClassifiedEvent {
                                job_id: job_id.clone(),
                                segment_id,
                                detected_class: cls,
                                person_count,
                            },
                        );
                    }
                    SidecarEvent::Progress {
                        segment_id,
                        current_frame,
                        total_frames,
                    } => {
                        let _ = app.emit(
                            "segment:progress",
                            serde_json::json!({
                                "job_id": job_id,
                                "segment_id": segment_id,
                                "current_frame": current_frame,
                                "total_frames": total_frames,
                            }),
                        );
                    }
                    SidecarEvent::TrackCompleted {
                        person_track_id,
                        clip_type,
                        fps,
                        frame_count,
                        quality_score,
                        has_hands,
                        has_face,
                        raw_perf_path,
                        ..
                    } => {
                        completed_tracks.push((
                            person_track_id,
                            clip_type,
                            fps,
                            frame_count,
                            quality_score,
                            has_hands,
                            has_face,
                            raw_perf_path,
                        ));
                    }
                    _ => {}
                })
                .await;

            match extract_res {
                Ok(_) => {
                    consecutive_failures = 0;
                    let cls_str = detected_class
                        .clone()
                        .unwrap_or_else(|| "full_performance".to_string());

                    if cls_str == "skipped_empty" || completed_tracks.is_empty() {
                        let db = self.db.lock().unwrap();
                        let _ = db.update_segment_status(&seg.id, "skipped", Some(&cls_str), None);
                        let _ = db.refresh_job_counters(&job_id);
                    } else {
                        // Stage 3: Compile each extracted track via Rust kinematics (< 15ms each)
                        for (
                            track_id,
                            clip_type,
                            fps,
                            raw_frames,
                            quality_score,
                            has_hands,
                            has_face,
                            raw_perf_path,
                        ) in completed_tracks
                        {
                            // Resume safety: prevent duplicate clips if already inserted before crash
                            let exists = {
                                let db = self.db.lock().unwrap();
                                db.clip_exists_for_segment_track(&job_id, seg.start_sec, track_id)
                                    .unwrap_or(false)
                            };
                            if exists {
                                continue;
                            }

                            let clip_id = Uuid::new_v4().to_string();
                            let polish = PolishConfig {
                                include_hands: has_hands,
                                include_face: has_face,
                                trim_start: 0,
                                trim_end: raw_frames.saturating_sub(1),
                                ..Default::default()
                            };

                            let raw_path = PathBuf::from(&raw_perf_path);
                            if let Ok(raw_data) = kinematics::load_raw_perf(&raw_path) {
                                if let Ok(compiled) = kinematics::compile_raw_performance(
                                    &raw_data,
                                    &polish,
                                    &vault_dir,
                                    &clip_id,
                                ) {
                                    let stem = Path::new(&job.source_name)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("clip");
                                    let label = if clip_type == "face_only" {
                                        format!("{} S{:02} Face", stem, seg.segment_index + 1)
                                    } else if track_id > 1 {
                                        format!(
                                            "{} S{:02} P{}",
                                            stem,
                                            seg.segment_index + 1,
                                            track_id
                                        )
                                    } else {
                                        format!("{} S{:02}", stem, seg.segment_index + 1)
                                    };

                                    let clip = MotionClip {
                                        id: clip_id,
                                        job_id: Some(job_id.clone()),
                                        collection_id: None,
                                        clip_type,
                                        person_track_id: track_id,
                                        title: label,
                                        source_video: job.source_name.clone(),
                                        source_start_sec: seg.start_sec,
                                        source_end_sec: seg.end_sec,
                                        fps,
                                        total_raw_frames: raw_frames as i64,
                                        frame_count: compiled.frame_count as i64,
                                        duration_sec: compiled.duration_sec,
                                        quality_score,
                                        has_hands,
                                        has_face,
                                        body_tags: compiled.body_tags,
                                        emotion_tags: compiled.emotion_tags,
                                        is_favorite: false,
                                        raw_perf_path,
                                        bvh_path: compiled
                                            .bvh_path
                                            .map(|p| p.to_string_lossy().to_string()),
                                        face_path: compiled
                                            .face_path
                                            .map(|p| p.to_string_lossy().to_string()),
                                        muscle_path: compiled
                                            .muscle_path
                                            .map(|p| p.to_string_lossy().to_string()),
                                        polish_config: polish,
                                        created_at: Utc::now().to_rfc3339(),
                                    };

                                    {
                                        let db = self.db.lock().unwrap();
                                        let _ = db.insert_clip(&clip);
                                    }
                                    let _ = app.emit("vault:clip_added", &clip);
                                }
                            }
                        }

                        let db = self.db.lock().unwrap();
                        let _ = db.update_segment_status(&seg.id, "completed", Some(&cls_str), None);
                        let _ = db.refresh_job_counters(&job_id);
                    }

                    // Clean up temporary video slice to keep disk usage lean
                    if temp_video.is_file() {
                        let _ = std::fs::remove_file(&temp_video);
                    }
                    {
                        let db = self.db.lock().unwrap();
                        let _ = db.clear_segment_temp_path(&seg.id);
                    }
                }
                Err(e) => {
                    consecutive_failures += 1;
                    let err_msg = e.to_string();
                    let db = self.db.lock().unwrap();
                    let _ = db.update_segment_status(
                        &seg.id,
                        "failed",
                        detected_class.as_deref(),
                        Some(&err_msg),
                    );
                    let _ = db.refresh_job_counters(&job_id);
                    // Reset sidecar so next segment spawns a fresh healthy worker
                    *sidecar = None;

                    if consecutive_failures >= 2 {
                        let _ = db.update_job_status(
                            &job_id,
                            "failed",
                            Some(&format!("Sidecar AI gagal berulang: {}", err_msg)),
                        );
                        drop(db);
                        emit_queue_update(&self.db, app);
                        return;
                    }
                }
            }

            emit_queue_update(&self.db, app);
        }
    }
}

pub fn emit_queue_update(db: &Arc<Mutex<VaultDb>>, app: &AppHandle) {
    let jobs = {
        let guard = db.lock().unwrap();
        guard.list_jobs().unwrap_or_default()
    };
    let _ = app.emit("queue:updated", jobs);
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::models::{HarvestConfig, JobSegment, VaultFilter};
    use tempfile::tempdir;

    /// Step 7 acceptance test: Pause -> Close App -> Reopen -> Resume without duplicating clips.
    #[test]
    fn test_pause_close_reopen_resume_no_duplicates() {
        let dir = tempdir().unwrap();
        let job_id: String;

        // Session 1: Create job with 3 segments, finish seg 0, start seg 1, then crash/close mid-flight
        {
            let db = VaultDb::open(dir.path()).unwrap();
            let job = db
                .create_job("D:/film.mp4", "film.mp4", "fast", &HarvestConfig::default())
                .unwrap();
            job_id = job.id.clone();
            db.update_job_status(&job_id, "processing", None).unwrap();

            let segs = vec![
                JobSegment {
                    id: "s0".into(),
                    job_id: job_id.clone(),
                    segment_index: 0,
                    start_sec: 0.0,
                    end_sec: 4.0,
                    temp_video_path: None,
                    detected_class: Some("full_performance".into()),
                    status: "completed".into(),
                    error_message: None,
                },
                JobSegment {
                    id: "s1".into(),
                    job_id: job_id.clone(),
                    segment_index: 1,
                    start_sec: 4.0,
                    end_sec: 8.0,
                    temp_video_path: None,
                    detected_class: None,
                    status: "extracting".into(),
                    error_message: None,
                },
                JobSegment {
                    id: "s2".into(),
                    job_id: job_id.clone(),
                    segment_index: 2,
                    start_sec: 8.0,
                    end_sec: 12.0,
                    temp_video_path: None,
                    detected_class: None,
                    status: "pending".into(),
                    error_message: None,
                },
            ];
            db.insert_segments(&segs).unwrap();

            // Clip for s0 already saved
            let clip0 = MotionClip {
                id: "clip-s0".into(),
                job_id: Some(job_id.clone()),
                collection_id: None,
                clip_type: "full_body".into(),
                person_track_id: 1,
                title: "film S01".into(),
                source_video: "film.mp4".into(),
                source_start_sec: 0.0,
                source_end_sec: 4.0,
                fps: 24.0,
                total_raw_frames: 96,
                frame_count: 96,
                duration_sec: 4.0,
                quality_score: 0.9,
                has_hands: true,
                has_face: true,
                body_tags: vec!["walk".into()],
                emotion_tags: vec![],
                is_favorite: false,
                raw_perf_path: "/raw/s0.rawperf.json".into(),
                bvh_path: Some("/bvh/s0.bvh".into()),
                face_path: None,
                muscle_path: None,
                polish_config: PolishConfig::default(),
                created_at: Utc::now().to_rfc3339(),
            };
            db.insert_clip(&clip0).unwrap();
        }

        // Session 2: Reopen VaultDb, run startup crash recovery, resume remaining segments
        {
            let db = VaultDb::open(dir.path()).unwrap();
            let (j_rec, s_rec) = db.recover_interrupted_jobs().unwrap();
            assert_eq!(j_rec, 1);
            assert_eq!(s_rec, 1);

            let job = db.get_job(&job_id).unwrap();
            assert_eq!(job.status, "paused");

            // Resume -> next pending must be s1 (NOT s0!), and s0 clip deduplication check returns true
            assert!(db.clip_exists_for_segment_track(&job_id, 0.0, 1).unwrap());
            assert!(!db.clip_exists_for_segment_track(&job_id, 4.0, 1).unwrap());

            let next = db.next_pending_segment(&job_id).unwrap().unwrap();
            assert_eq!(next.id, "s1");

            db.update_segment_status("s1", "completed", Some("face_closeup"), None)
                .unwrap();
            db.update_segment_status("s2", "skipped", Some("skipped_empty"), None)
                .unwrap();
            let refreshed = db.refresh_job_counters(&job_id).unwrap();
            assert_eq!(refreshed.total_segments, 3);
            assert_eq!(refreshed.done_segments, 3);
            assert_eq!(refreshed.face_segments, 1);
            assert_eq!(refreshed.skipped_segments, 1);
            assert!(db.next_pending_segment(&job_id).unwrap().is_none());

            // No duplicate clips created for s0
            assert_eq!(db.query_vault(&VaultFilter::default()).unwrap().len(), 1);
        }
    }
}
