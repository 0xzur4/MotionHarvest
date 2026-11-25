use crate::models::{
    AppSettings, BatchJob, Collection, HarvestConfig, JobSegment, MotionClip, PolishConfig,
    VaultFilter,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type DbResult<T> = Result<T, DbError>;

pub struct VaultDb {
    conn: Connection,
    vault_dir: PathBuf,
}

impl VaultDb {
    /// Initializes vault subdirectories (`raw/`, `bvh/`, `face/`, `muscle/`, `models/`, `temp/`)
    /// and opens `<vault_dir>/vault.db` in WAL mode with foreign keys enabled.
    pub fn open(vault_dir: &Path) -> DbResult<Self> {
        ensure_vault_dirs(vault_dir)?;
        let db_path = vault_dir.join("vault.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA synchronous = NORMAL;
            ",
        )?;
        let db = Self {
            conn,
            vault_dir: vault_dir.to_path_buf(),
        };
        db.init_schema()?;
        db.ensure_default_settings()?;
        Ok(db)
    }

    pub fn vault_dir(&self) -> &Path {
        &self.vault_dir
    }

    fn init_schema(&self) -> DbResult<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS batch_jobs (
                id TEXT PRIMARY KEY,
                source_path TEXT NOT NULL,
                source_name TEXT NOT NULL,
                source_duration_sec REAL NOT NULL DEFAULT 0,
                ai_mode TEXT NOT NULL DEFAULT 'fast',
                config_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                total_segments INTEGER NOT NULL DEFAULT 0,
                done_segments INTEGER NOT NULL DEFAULT 0,
                face_segments INTEGER NOT NULL DEFAULT 0,
                skipped_segments INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS job_segments (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL REFERENCES batch_jobs(id) ON DELETE CASCADE,
                segment_index INTEGER NOT NULL,
                start_sec REAL NOT NULL,
                end_sec REAL NOT NULL,
                temp_video_path TEXT,
                detected_class TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                error_message TEXT
            );

            CREATE TABLE IF NOT EXISTS motion_clips (
                id TEXT PRIMARY KEY,
                job_id TEXT REFERENCES batch_jobs(id) ON DELETE SET NULL,
                collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
                clip_type TEXT NOT NULL DEFAULT 'full_body',
                person_track_id INTEGER NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                source_video TEXT NOT NULL,
                source_start_sec REAL NOT NULL,
                source_end_sec REAL NOT NULL,
                fps REAL NOT NULL,
                total_raw_frames INTEGER NOT NULL,
                frame_count INTEGER NOT NULL,
                duration_sec REAL NOT NULL,
                quality_score REAL NOT NULL,
                has_hands INTEGER NOT NULL DEFAULT 0,
                has_face INTEGER NOT NULL DEFAULT 0,
                body_tags TEXT NOT NULL DEFAULT '[]',
                emotion_tags TEXT NOT NULL DEFAULT '[]',
                is_favorite INTEGER NOT NULL DEFAULT 0,
                raw_perf_path TEXT NOT NULL,
                bvh_path TEXT,
                face_path TEXT,
                muscle_path TEXT,
                polish_config_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_job_segments_job_id ON job_segments(job_id);
            CREATE INDEX IF NOT EXISTS idx_motion_clips_type ON motion_clips(clip_type);
            CREATE INDEX IF NOT EXISTS idx_motion_clips_collection ON motion_clips(collection_id);
            CREATE INDEX IF NOT EXISTS idx_motion_clips_favorite ON motion_clips(is_favorite);
            ",
        )?;
        Ok(())
    }

    fn ensure_default_settings(&self) -> DbResult<()> {
        let defaults = [
            ("vault_dir", self.vault_dir.to_string_lossy().to_string()),
            ("blender_port", "9877".to_string()),
            ("default_ai_mode", "fast".to_string()),
            ("scene_threshold", "12.0".to_string()),
            ("use_nvenc", "auto".to_string()),
        ];
        for (k, v) in defaults {
            self.conn.execute(
                "INSERT OR IGNORE INTO app_settings (key, value) VALUES (?1, ?2)",
                params![k, v],
            )?;
        }
        Ok(())
    }

    pub fn get_settings(&self) -> DbResult<AppSettings> {
        let mut settings =
            AppSettings::default_with_vault(self.vault_dir.to_string_lossy().to_string());
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM app_settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (k, v) = row?;
            match k.as_str() {
                "vault_dir" => settings.vault_dir = v,
                "blender_port" => {
                    if let Ok(port) = v.parse::<u16>() {
                        settings.blender_port = port;
                    }
                }
                "default_ai_mode" => settings.default_ai_mode = v,
                "scene_threshold" => {
                    if let Ok(th) = v.parse::<f64>() {
                        settings.scene_threshold = th;
                    }
                }
                "use_nvenc" => settings.use_nvenc = v,
                _ => {}
            }
        }
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> DbResult<()> {
        let pairs = [
            ("vault_dir", settings.vault_dir.clone()),
            ("blender_port", settings.blender_port.to_string()),
            ("default_ai_mode", settings.default_ai_mode.clone()),
            ("scene_threshold", settings.scene_threshold.to_string()),
            ("use_nvenc", settings.use_nvenc.clone()),
        ];
        for (k, v) in pairs {
            self.conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![k, v],
            )?;
        }
        Ok(())
    }

    /// Startup crash recovery:
    /// - Resets any `splitting` or `processing` jobs to `paused` so user can resume cleanly.
    /// - Resets any `extracting` segments back to `pending` so no segment is lost or duplicated.
    pub fn recover_interrupted_jobs(&self) -> DbResult<(usize, usize)> {
        let now = Utc::now().to_rfc3339();
        let jobs_recovered = self.conn.execute(
            "UPDATE batch_jobs
             SET status = 'paused', updated_at = ?1
             WHERE status IN ('splitting', 'processing')",
            params![now],
        )?;
        let segs_recovered = self.conn.execute(
            "UPDATE job_segments
             SET status = 'pending'
             WHERE status = 'extracting'",
            [],
        )?;
        Ok((jobs_recovered, segs_recovered))
    }

    // ---- Batch Jobs CRUD ----

    pub fn create_job(
        &self,
        source_path: &str,
        source_name: &str,
        ai_mode: &str,
        config: &HarvestConfig,
    ) -> DbResult<BatchJob> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let config_json = serde_json::to_string(config)?;
        self.conn.execute(
            "INSERT INTO batch_jobs (
                id, source_path, source_name, source_duration_sec, ai_mode,
                config_json, status, total_segments, done_segments,
                face_segments, skipped_segments, error_message, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 0.0, ?4, ?5, 'queued', 0, 0, 0, 0, NULL, ?6, ?6)",
            params![id, source_path, source_name, ai_mode, config_json, now],
        )?;
        self.get_job(&id)
    }

    pub fn get_job(&self, job_id: &str) -> DbResult<BatchJob> {
        self.conn
            .query_row(
                "SELECT id, source_path, source_name, source_duration_sec, ai_mode, config_json,
                        status, total_segments, done_segments, face_segments, skipped_segments,
                        error_message, created_at, updated_at
                 FROM batch_jobs WHERE id = ?1",
                params![job_id],
                row_to_job,
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("batch_job {}", job_id)))
    }

    pub fn list_jobs(&self) -> DbResult<Vec<BatchJob>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_path, source_name, source_duration_sec, ai_mode, config_json,
                    status, total_segments, done_segments, face_segments, skipped_segments,
                    error_message, created_at, updated_at
             FROM batch_jobs
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_job)?;
        let mut jobs = Vec::new();
        for r in rows {
            jobs.push(r?);
        }
        Ok(jobs)
    }

    pub fn next_runnable_job(&self) -> DbResult<Option<BatchJob>> {
        self.conn
            .query_row(
                "SELECT id, source_path, source_name, source_duration_sec, ai_mode, config_json,
                        status, total_segments, done_segments, face_segments, skipped_segments,
                        error_message, created_at, updated_at
                 FROM batch_jobs
                 WHERE status IN ('processing', 'splitting', 'queued')
                 ORDER BY CASE status WHEN 'processing' THEN 0 WHEN 'splitting' THEN 1 ELSE 2 END,
                          created_at ASC
                 LIMIT 1",
                [],
                row_to_job,
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn update_job_status(
        &self,
        job_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> DbResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE batch_jobs SET status = ?1, error_message = ?2, updated_at = ?3 WHERE id = ?4",
            params![status, error_message, now, job_id],
        )?;
        Ok(())
    }

    pub fn update_job_metadata(
        &self,
        job_id: &str,
        source_duration_sec: f64,
        total_segments: i64,
    ) -> DbResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE batch_jobs
             SET source_duration_sec = ?1, total_segments = ?2, updated_at = ?3
             WHERE id = ?4",
            params![source_duration_sec, total_segments, now, job_id],
        )?;
        Ok(())
    }

    pub fn refresh_job_counters(&self, job_id: &str) -> DbResult<BatchJob> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE batch_jobs
             SET total_segments = (SELECT COUNT(*) FROM job_segments WHERE job_id = ?1),
                 done_segments = (SELECT COUNT(*) FROM job_segments WHERE job_id = ?1 AND status IN ('completed', 'skipped', 'failed')),
                 face_segments = (SELECT COUNT(*) FROM job_segments WHERE job_id = ?1 AND detected_class = 'face_closeup' AND status = 'completed'),
                 skipped_segments = (SELECT COUNT(*) FROM job_segments WHERE job_id = ?1 AND status = 'skipped'),
                 updated_at = ?2
             WHERE id = ?1",
            params![job_id, now],
        )?;
        self.get_job(job_id)
    }

    pub fn delete_job(&self, job_id: &str) -> DbResult<()> {
        self.conn
            .execute("DELETE FROM batch_jobs WHERE id = ?1", params![job_id])?;
        Ok(())
    }

    // ---- Job Segments CRUD ----

    pub fn insert_segments(&self, segments: &[JobSegment]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO job_segments (
                    id, job_id, segment_index, start_sec, end_sec,
                    temp_video_path, detected_class, status, error_message
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for seg in segments {
                stmt.execute(params![
                    seg.id,
                    seg.job_id,
                    seg.segment_index,
                    seg.start_sec,
                    seg.end_sec,
                    seg.temp_video_path,
                    seg.detected_class,
                    seg.status,
                    seg.error_message,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_segments_for_job(&self, job_id: &str) -> DbResult<Vec<JobSegment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, job_id, segment_index, start_sec, end_sec,
                    temp_video_path, detected_class, status, error_message
             FROM job_segments
             WHERE job_id = ?1
             ORDER BY segment_index ASC",
        )?;
        let rows = stmt.query_map(params![job_id], row_to_segment)?;
        let mut segs = Vec::new();
        for r in rows {
            segs.push(r?);
        }
        Ok(segs)
    }

    pub fn next_pending_segment(&self, job_id: &str) -> DbResult<Option<JobSegment>> {
        self.conn
            .query_row(
                "SELECT id, job_id, segment_index, start_sec, end_sec,
                        temp_video_path, detected_class, status, error_message
                 FROM job_segments
                 WHERE job_id = ?1 AND status = 'pending'
                 ORDER BY segment_index ASC
                 LIMIT 1",
                params![job_id],
                row_to_segment,
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn update_segment_status(
        &self,
        segment_id: &str,
        status: &str,
        detected_class: Option<&str>,
        error_message: Option<&str>,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE job_segments
             SET status = ?1,
                 detected_class = COALESCE(?2, detected_class),
                 error_message = ?3
             WHERE id = ?4",
            params![status, detected_class, error_message, segment_id],
        )?;
        Ok(())
    }

    pub fn clear_segment_temp_path(&self, segment_id: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE job_segments SET temp_video_path = NULL WHERE id = ?1",
            params![segment_id],
        )?;
        Ok(())
    }

    // ---- Collections CRUD ----

    pub fn create_collection(&self, name: &str) -> DbResult<Collection> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name.trim(), now],
        )?;
        Ok(Collection {
            id,
            name: name.trim().to_string(),
            created_at: now,
            clip_count: 0,
        })
    }

    pub fn list_collections(&self) -> DbResult<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.created_at, COUNT(m.id) AS clip_count
             FROM collections c
             LEFT JOIN motion_clips m ON m.collection_id = c.id
             GROUP BY c.id
             ORDER BY c.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                clip_count: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_collection(&self, collection_id: &str) -> DbResult<()> {
        self.conn.execute(
            "DELETE FROM collections WHERE id = ?1",
            params![collection_id],
        )?;
        Ok(())
    }

    pub fn assign_clips_to_collection(
        &self,
        clip_ids: &[String],
        collection_id: Option<&str>,
    ) -> DbResult<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut updated = 0usize;
        {
            let mut stmt = tx.prepare(
                "UPDATE motion_clips SET collection_id = ?1 WHERE id = ?2",
            )?;
            for cid in clip_ids {
                updated += stmt.execute(params![collection_id, cid])?;
            }
        }
        tx.commit()?;
        Ok(updated)
    }

    // ---- Motion Clips CRUD & Vault Query ----

    pub fn insert_clip(&self, clip: &MotionClip) -> DbResult<()> {
        let body_tags_json = serde_json::to_string(&clip.body_tags)?;
        let emotion_tags_json = serde_json::to_string(&clip.emotion_tags)?;
        let polish_json = serde_json::to_string(&clip.polish_config)?;
        self.conn.execute(
            "INSERT INTO motion_clips (
                id, job_id, collection_id, clip_type, person_track_id, title,
                source_video, source_start_sec, source_end_sec, fps,
                total_raw_frames, frame_count, duration_sec, quality_score,
                has_hands, has_face, body_tags, emotion_tags, is_favorite,
                raw_perf_path, bvh_path, face_path, muscle_path, polish_config_json, created_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19,
                ?20, ?21, ?22, ?23, ?24, ?25
             )",
            params![
                clip.id,
                clip.job_id,
                clip.collection_id,
                clip.clip_type,
                clip.person_track_id,
                clip.title,
                clip.source_video,
                clip.source_start_sec,
                clip.source_end_sec,
                clip.fps,
                clip.total_raw_frames,
                clip.frame_count,
                clip.duration_sec,
                clip.quality_score,
                clip.has_hands as i32,
                clip.has_face as i32,
                body_tags_json,
                emotion_tags_json,
                clip.is_favorite as i32,
                clip.raw_perf_path,
                clip.bvh_path,
                clip.face_path,
                clip.muscle_path,
                polish_json,
                clip.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_clip(&self, clip_id: &str) -> DbResult<MotionClip> {
        self.conn
            .query_row(
                "SELECT id, job_id, collection_id, clip_type, person_track_id, title,
                        source_video, source_start_sec, source_end_sec, fps,
                        total_raw_frames, frame_count, duration_sec, quality_score,
                        has_hands, has_face, body_tags, emotion_tags, is_favorite,
                        raw_perf_path, bvh_path, face_path, muscle_path, polish_config_json, created_at
                 FROM motion_clips WHERE id = ?1",
                params![clip_id],
                row_to_clip,
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("motion_clip {}", clip_id)))
    }

    pub fn update_clip_compiled(
        &self,
        clip_id: &str,
        frame_count: i64,
        duration_sec: f64,
        body_tags: &[String],
        emotion_tags: &[String],
        bvh_path: Option<&str>,
        face_path: Option<&str>,
        muscle_path: Option<&str>,
        polish_config: &PolishConfig,
    ) -> DbResult<MotionClip> {
        let body_tags_json = serde_json::to_string(body_tags)?;
        let emotion_tags_json = serde_json::to_string(emotion_tags)?;
        let polish_json = serde_json::to_string(polish_config)?;
        self.conn.execute(
            "UPDATE motion_clips
             SET frame_count = ?1,
                 duration_sec = ?2,
                 body_tags = ?3,
                 emotion_tags = ?4,
                 bvh_path = ?5,
                 face_path = ?6,
                 muscle_path = ?7,
                 polish_config_json = ?8
             WHERE id = ?9",
            params![
                frame_count,
                duration_sec,
                body_tags_json,
                emotion_tags_json,
                bvh_path,
                face_path,
                muscle_path,
                polish_json,
                clip_id
            ],
        )?;
        self.get_clip(clip_id)
    }

    pub fn rename_clip(&self, clip_id: &str, title: &str) -> DbResult<MotionClip> {
        self.conn.execute(
            "UPDATE motion_clips SET title = ?1 WHERE id = ?2",
            params![title.trim(), clip_id],
        )?;
        self.get_clip(clip_id)
    }

    pub fn toggle_favorite(&self, clip_id: &str) -> DbResult<MotionClip> {
        self.conn.execute(
            "UPDATE motion_clips SET is_favorite = CASE WHEN is_favorite = 1 THEN 0 ELSE 1 END WHERE id = ?1",
            params![clip_id],
        )?;
        self.get_clip(clip_id)
    }

    pub fn delete_clips(&self, clip_ids: &[String]) -> DbResult<Vec<MotionClip>> {
        let mut deleted = Vec::new();
        for cid in clip_ids {
            if let Ok(clip) = self.get_clip(cid) {
                self.conn
                    .execute("DELETE FROM motion_clips WHERE id = ?1", params![cid])?;
                deleted.push(clip);
            }
        }
        Ok(deleted)
    }

    pub fn query_vault(&self, filter: &VaultFilter) -> DbResult<Vec<MotionClip>> {
        let mut sql = String::from(
            "SELECT id, job_id, collection_id, clip_type, person_track_id, title,
                    source_video, source_start_sec, source_end_sec, fps,
                    total_raw_frames, frame_count, duration_sec, quality_score,
                    has_hands, has_face, body_tags, emotion_tags, is_favorite,
                    raw_perf_path, bvh_path, face_path, muscle_path, polish_config_json, created_at
             FROM motion_clips WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(ref ct) = filter.clip_type {
            let trimmed = ct.trim();
            if !trimmed.is_empty() && trimmed != "all" {
                sql.push_str(" AND clip_type = ?");
                values.push(rusqlite::types::Value::Text(trimmed.to_string()));
            }
        }
        if let Some(ref cid) = filter.collection_id {
            let trimmed = cid.trim();
            if !trimmed.is_empty() && trimmed != "all" {
                sql.push_str(" AND collection_id = ?");
                values.push(rusqlite::types::Value::Text(trimmed.to_string()));
            }
        }
        if filter.favorites_only.unwrap_or(false) {
            sql.push_str(" AND is_favorite = 1");
        }
        if let Some(ref btag) = filter.body_tag {
            let trimmed = btag.trim();
            if !trimmed.is_empty() && trimmed != "all" {
                sql.push_str(" AND body_tags LIKE ?");
                values.push(rusqlite::types::Value::Text(format!("%\"{}\"%", trimmed)));
            }
        }
        if let Some(ref etag) = filter.emotion_tag {
            let trimmed = etag.trim();
            if !trimmed.is_empty() && trimmed != "all" {
                sql.push_str(" AND emotion_tags LIKE ?");
                values.push(rusqlite::types::Value::Text(format!("%\"{}\"%", trimmed)));
            }
        }
        if let Some(ref q) = filter.search {
            let trimmed = q.trim();
            if !trimmed.is_empty() {
                let pat = format!("%{}%", trimmed);
                sql.push_str(
                    " AND (title LIKE ? OR source_video LIKE ? OR body_tags LIKE ? OR emotion_tags LIKE ?)",
                );
                values.push(rusqlite::types::Value::Text(pat.clone()));
                values.push(rusqlite::types::Value::Text(pat.clone()));
                values.push(rusqlite::types::Value::Text(pat.clone()));
                values.push(rusqlite::types::Value::Text(pat));
            }
        }

        sql.push_str(" ORDER BY created_at DESC");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), row_to_clip)?;
        let mut clips = Vec::new();
        for r in rows {
            clips.push(r?);
        }
        Ok(clips)
    }

    pub fn clip_exists_for_segment_track(
        &self,
        job_id: &str,
        source_start_sec: f64,
        person_track_id: i64,
    ) -> DbResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM motion_clips
             WHERE job_id = ?1 AND ABS(source_start_sec - ?2) < 0.05 AND person_track_id = ?3",
            params![job_id, source_start_sec, person_track_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

pub fn ensure_vault_dirs(vault_dir: &Path) -> std::io::Result<()> {
    for sub in ["raw", "bvh", "face", "muscle", "models", "temp"] {
        std::fs::create_dir_all(vault_dir.join(sub))?;
    }
    Ok(())
}

pub fn default_vault_dir() -> PathBuf {
    if let Some(docs) = dirs::document_dir() {
        return docs.join("MotionHarvestVault");
    }
    if let Some(home) = dirs::home_dir() {
        return home.join("MotionHarvestVault");
    }
    PathBuf::from("MotionHarvestVault")
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<BatchJob> {
    let config_str: String = row.get(5)?;
    let config: HarvestConfig = serde_json::from_str(&config_str).unwrap_or_default();
    Ok(BatchJob {
        id: row.get(0)?,
        source_path: row.get(1)?,
        source_name: row.get(2)?,
        source_duration_sec: row.get(3)?,
        ai_mode: row.get(4)?,
        config,
        status: row.get(6)?,
        total_segments: row.get(7)?,
        done_segments: row.get(8)?,
        face_segments: row.get(9)?,
        skipped_segments: row.get(10)?,
        error_message: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn row_to_segment(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobSegment> {
    Ok(JobSegment {
        id: row.get(0)?,
        job_id: row.get(1)?,
        segment_index: row.get(2)?,
        start_sec: row.get(3)?,
        end_sec: row.get(4)?,
        temp_video_path: row.get(5)?,
        detected_class: row.get(6)?,
        status: row.get(7)?,
        error_message: row.get(8)?,
    })
}

fn row_to_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<MotionClip> {
    let body_tags_str: String = row.get(16)?;
    let emotion_tags_str: String = row.get(17)?;
    let polish_str: String = row.get(23)?;
    Ok(MotionClip {
        id: row.get(0)?,
        job_id: row.get(1)?,
        collection_id: row.get(2)?,
        clip_type: row.get(3)?,
        person_track_id: row.get(4)?,
        title: row.get(5)?,
        source_video: row.get(6)?,
        source_start_sec: row.get(7)?,
        source_end_sec: row.get(8)?,
        fps: row.get(9)?,
        total_raw_frames: row.get(10)?,
        frame_count: row.get(11)?,
        duration_sec: row.get(12)?,
        quality_score: row.get(13)?,
        has_hands: row.get::<_, i32>(14)? != 0,
        has_face: row.get::<_, i32>(15)? != 0,
        body_tags: serde_json::from_str(&body_tags_str).unwrap_or_default(),
        emotion_tags: serde_json::from_str(&emotion_tags_str).unwrap_or_default(),
        is_favorite: row.get::<_, i32>(18)? != 0,
        raw_perf_path: row.get(19)?,
        bvh_path: row.get(20)?,
        face_path: row.get(21)?,
        muscle_path: row.get(22)?,
        polish_config: serde_json::from_str(&polish_str).unwrap_or_default(),
        created_at: row.get(24)?,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_clip(
        id: &str,
        clip_type: &str,
        title: &str,
        body_tags: Vec<&str>,
        emotion_tags: Vec<&str>,
        collection_id: Option<String>,
        fav: bool,
    ) -> MotionClip {
        MotionClip {
            id: id.to_string(),
            job_id: None,
            collection_id,
            clip_type: clip_type.to_string(),
            person_track_id: 1,
            title: title.to_string(),
            source_video: "anime_ep04.mkv".to_string(),
            source_start_sec: 12.0,
            source_end_sec: 16.2,
            fps: 24.0,
            total_raw_frames: 100,
            frame_count: 100,
            duration_sec: 4.2,
            quality_score: 0.94,
            has_hands: clip_type != "face_only",
            has_face: true,
            body_tags: body_tags.into_iter().map(String::from).collect(),
            emotion_tags: emotion_tags.into_iter().map(String::from).collect(),
            is_favorite: fav,
            raw_perf_path: format!("/vault/raw/{}.rawperf.json", id),
            bvh_path: if clip_type == "face_only" {
                None
            } else {
                Some(format!("/vault/bvh/{}.bvh", id))
            },
            face_path: Some(format!("/vault/face/{}.face.json", id)),
            muscle_path: Some(format!("/vault/muscle/{}.muscle.json", id)),
            polish_config: PolishConfig::default(),
            created_at: Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_vault_dirs_and_settings_crud() {
        let dir = tempdir().unwrap();
        let db = VaultDb::open(dir.path()).unwrap();

        for sub in ["raw", "bvh", "face", "muscle", "models", "temp"] {
            assert!(dir.path().join(sub).is_dir());
        }

        let mut s = db.get_settings().unwrap();
        assert_eq!(s.blender_port, 9877);
        assert_eq!(s.default_ai_mode, "fast");

        s.blender_port = 9988;
        s.default_ai_mode = "anime_2d".to_string();
        db.save_settings(&s).unwrap();

        let reloaded = db.get_settings().unwrap();
        assert_eq!(reloaded.blender_port, 9988);
        assert_eq!(reloaded.default_ai_mode, "anime_2d");
    }

    #[test]
    fn test_clip_crud_and_combined_filters() {
        let dir = tempdir().unwrap();
        let db = VaultDb::open(dir.path()).unwrap();

        let col = db.create_collection("Action Pack").unwrap();
        let c1 = sample_clip(
            "clip-1",
            "full_body",
            "Lari + Kepal",
            vec!["run", "action"],
            vec!["angry"],
            Some(col.id.clone()),
            true,
        );
        let c2 = sample_clip(
            "clip-2",
            "face_only",
            "Senyum CloseUp",
            vec![],
            vec!["happy"],
            None,
            false,
        );
        let c3 = sample_clip(
            "clip-3",
            "blended",
            "Jalan lalu Lompat",
            vec!["walk", "jump"],
            vec!["happy", "talking"],
            Some(col.id.clone()),
            true,
        );

        db.insert_clip(&c1).unwrap();
        db.insert_clip(&c2).unwrap();
        db.insert_clip(&c3).unwrap();

        // Filter combination: clip_type + body_tag + emotion_tag
        let res = db
            .query_vault(&VaultFilter {
                clip_type: Some("full_body".into()),
                body_tag: Some("run".into()),
                emotion_tag: Some("angry".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "clip-1");

        // Filter face_only + happy
        let res_face = db
            .query_vault(&VaultFilter {
                clip_type: Some("face_only".into()),
                emotion_tag: Some("happy".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_face.len(), 1);
        assert_eq!(res_face[0].id, "clip-2");

        // Toggle favorite
        let toggled = db.toggle_favorite("clip-2").unwrap();
        assert!(toggled.is_favorite);

        // Search query + collection
        let res_search = db
            .query_vault(&VaultFilter {
                search: Some("Lompat".into()),
                collection_id: Some(col.id.clone()),
                favorites_only: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_search.len(), 1);
        assert_eq!(res_search[0].id, "clip-3");

        // Collections clip_count
        let cols = db.list_collections().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].clip_count, 2);

        // Delete clip
        let deleted = db.delete_clips(&["clip-2".into()]).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(db.query_vault(&VaultFilter::default()).unwrap().len(), 2);
    }

    #[test]
    fn test_recover_interrupted_jobs() {
        let dir = tempdir().unwrap();
        let db = VaultDb::open(dir.path()).unwrap();

        let job = db
            .create_job("D:/video.mp4", "video.mp4", "anime_2d", &HarvestConfig::default())
            .unwrap();
        db.update_job_status(&job.id, "processing", None).unwrap();

        let segs = vec![
            JobSegment {
                id: "seg-1".into(),
                job_id: job.id.clone(),
                segment_index: 0,
                start_sec: 0.0,
                end_sec: 4.0,
                temp_video_path: None,
                detected_class: Some("full_performance".into()),
                status: "completed".into(),
                error_message: None,
            },
            JobSegment {
                id: "seg-2".into(),
                job_id: job.id.clone(),
                segment_index: 1,
                start_sec: 4.0,
                end_sec: 8.0,
                temp_video_path: Some("/tmp/seg2.mp4".into()),
                detected_class: None,
                status: "extracting".into(),
                error_message: None,
            },
        ];
        db.insert_segments(&segs).unwrap();

        let (jobs_rec, segs_rec) = db.recover_interrupted_jobs().unwrap();
        assert_eq!(jobs_rec, 1);
        assert_eq!(segs_rec, 1);

        let recovered_job = db.get_job(&job.id).unwrap();
        assert_eq!(recovered_job.status, "paused");

        let next_seg = db.next_pending_segment(&job.id).unwrap().unwrap();
        assert_eq!(next_seg.id, "seg-2");
        assert_eq!(next_seg.status, "pending");
    }
}
