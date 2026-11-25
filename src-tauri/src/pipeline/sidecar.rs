use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("Sidecar script worker.py tidak ditemukan")]
    ScriptNotFound,
    #[error("Gagal menjalankan Python sidecar: {0}")]
    Spawn(std::io::Error),
    #[error("Sidecar IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Sidecar error: {0}")]
    Protocol(String),
    #[error("Proses Python sidecar berhenti mendadak: {0}")]
    ProcessExited(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractRequest {
    pub cmd: String,
    pub segment_id: String,
    pub video_path: String,
    pub ai_mode: String,
    pub multi_person: bool,
    pub max_persons: u32,
    pub min_visibility: f64,
    pub output_dir: String,
    pub models_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SidecarEvent {
    Ready {
        #[serde(default)]
        version: String,
    },
    Classified {
        segment_id: String,
        #[serde(rename = "class")]
        detected_class: String,
        #[serde(default)]
        person_count: usize,
    },
    Progress {
        segment_id: String,
        current_frame: usize,
        total_frames: usize,
    },
    TrackCompleted {
        segment_id: String,
        person_track_id: i64,
        clip_type: String,
        fps: f64,
        frame_count: usize,
        quality_score: f64,
        has_hands: bool,
        has_face: bool,
        raw_perf_path: String,
    },
    SegmentDone {
        segment_id: String,
        tracks_produced: usize,
    },
    Error {
        #[serde(default)]
        segment_id: String,
        message: String,
    },
}

/// Resolves `uv` executable from PATH or WinGet Packages on Windows.
fn resolve_uv_bin() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { "uv.exe" } else { "uv" };
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let c = dir.join(exe_name);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let links = PathBuf::from(&local_app)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join(exe_name);
            if links.is_file() {
                return Some(links);
            }
            let pkgs = PathBuf::from(&local_app)
                .join("Microsoft")
                .join("WinGet")
                .join("Packages");
            if let Ok(entries) = std::fs::read_dir(&pkgs) {
                for ent in entries.flatten() {
                    let fname = ent.file_name().to_string_lossy().to_ascii_lowercase();
                    if fname.contains("astral-sh.uv") {
                        for walk in walkdir::WalkDir::new(ent.path())
                            .max_depth(3)
                            .into_iter()
                            .flatten()
                        {
                            if walk
                                .file_name()
                                .to_string_lossy()
                                .eq_ignore_ascii_case(exe_name)
                            {
                                return Some(walk.into_path());
                            }
                        }
                    }
                }
            }
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let c = PathBuf::from(profile).join(".local").join("bin").join(exe_name);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

fn venv_python_in(venv_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    }
}

/// Finds a ready Python interpreter with sidecar dependencies, or silently bootstraps
/// `<vault_dir>/sidecar-venv` via `uv sync` (with `CREATE_NO_WINDOW`) for installed NSIS builds.
async fn resolve_sidecar_python(sidecar_dir: &Path, vault_dir: &Path) -> Result<PathBuf, SidecarError> {
    // 1. Local dev venv inside sidecar-pose/.venv
    let local_venv = venv_python_in(&sidecar_dir.join(".venv"));
    if local_venv.is_file() {
        return Ok(local_venv);
    }

    // 2. Persistent standalone venv under <vault_dir>/sidecar-venv (for installed .exe)
    let vault_venv_dir = vault_dir.join("sidecar-venv");
    let vault_venv_py = venv_python_in(&vault_venv_dir);
    if vault_venv_py.is_file() {
        return Ok(vault_venv_py);
    }

    // 3. Auto-bootstrap <vault_dir>/sidecar-venv silently via `uv sync` if pyproject.toml & uv exist
    let pyproject = sidecar_dir.join("pyproject.toml");
    if pyproject.is_file() {
        if let Some(uv_bin) = resolve_uv_bin() {
            let mut uv_cmd = Command::new(&uv_bin);
            uv_cmd
                .args(["sync", "--project"])
                .arg(sidecar_dir)
                .args(["--python", "3.12", "--no-dev"])
                .env("UV_PROJECT_ENVIRONMENT", &vault_venv_dir)
                .env("UV_LINK_MODE", "copy")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            #[cfg(target_os = "windows")]
            {
                uv_cmd.creation_flags(CREATE_NO_WINDOW);
            }

            let out = uv_cmd.output().await.map_err(SidecarError::Spawn)?;
            if out.status.success() && vault_venv_py.is_file() {
                return Ok(vault_venv_py);
            }
            let err_txt = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(SidecarError::Protocol(format!(
                "Gagal menyiapkan Python AI environment via uv: {}",
                err_txt.chars().take(300).collect::<String>()
            )));
        }
    }

    // 4. Last fallback: system python
    Ok(PathBuf::from("python"))
}

/// Persistent warm Python worker communicating via line-delimited JSON (NDJSON) over stdin/stdout.
pub struct WarmSidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WarmSidecar {
    pub async fn spawn(sidecar_dir: &Path, vault_dir: &Path) -> Result<Self, SidecarError> {
        let worker_py = sidecar_dir.join("worker.py");
        if !worker_py.is_file() {
            return Err(SidecarError::ScriptNotFound);
        }

        let py_exe = resolve_sidecar_python(sidecar_dir, vault_dir).await?;
        let mpl_dir = vault_dir.join("temp").join("mpl");
        let _ = std::fs::create_dir_all(&mpl_dir);

        let mut cmd = Command::new(&py_exe);
        cmd.arg("-u")
            .arg(&worker_py)
            .current_dir(sidecar_dir)
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("MPLCONFIGDIR", &mpl_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(SidecarError::Spawn)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| SidecarError::Protocol("Missing stdin pipe".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SidecarError::Protocol("Missing stdout pipe".into()))?;
        let mut stderr_pipe = child.stderr.take();

        let mut sidecar = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };

        // Wait for initial `ready` handshake (ignoring non-JSON startup notices if any)
        let mut line = String::new();
        loop {
            line.clear();
            let n = sidecar.stdout.read_line(&mut line).await?;
            if n == 0 {
                let mut err_buf = String::new();
                if let Some(ref mut se) = stderr_pipe {
                    let _ = se.read_to_string(&mut err_buf).await;
                }
                let snippet: String = err_buf.trim().chars().take(350).collect();
                return Err(SidecarError::ProcessExited(if snippet.is_empty() {
                    "Python worker keluar sebelum siap".into()
                } else {
                    snippet
                }));
            }
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                if let Ok(SidecarEvent::Ready { .. }) = serde_json::from_str::<SidecarEvent>(trimmed)
                {
                    break;
                }
            }
        }

        // Drain stderr asynchronously so pipe buffer never blocks long-running worker
        if let Some(mut se) = stderr_pipe {
            tokio::spawn(async move {
                let mut sink = Vec::new();
                let _ = se.read_to_end(&mut sink).await;
            });
        }

        Ok(sidecar)
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Sends one `extract` NDJSON request and streams every `SidecarEvent` until `SegmentDone` or `Error`.
    pub async fn extract_segment<F>(
        &mut self,
        req: &ExtractRequest,
        mut on_event: F,
    ) -> Result<usize, SidecarError>
    where
        F: FnMut(SidecarEvent),
    {
        let mut payload =
            serde_json::to_string(req).map_err(|e| SidecarError::Protocol(e.to_string()))?;
        payload.push('\n');
        self.stdin.write_all(payload.as_bytes()).await?;
        self.stdin.flush().await?;

        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).await?;
            if n == 0 {
                return Err(SidecarError::ProcessExited(
                    "Worker berhenti saat memproses segmen".into(),
                ));
            }
            let trimmed = line.trim();
            if !trimmed.starts_with('{') {
                continue;
            }
            let evt: SidecarEvent = serde_json::from_str(trimmed)
                .map_err(|e| SidecarError::Protocol(format!("{}: {}", e, trimmed)))?;

            match &evt {
                SidecarEvent::SegmentDone {
                    tracks_produced, ..
                } => {
                    let count = *tracks_produced;
                    on_event(evt);
                    return Ok(count);
                }
                SidecarEvent::Error { message, .. } => {
                    let msg = message.clone();
                    on_event(evt);
                    return Err(SidecarError::Protocol(msg));
                }
                _ => {
                    on_event(evt);
                }
            }
        }
    }

    pub async fn shutdown(&mut self) {
        let _ = self.stdin.write_all(b"{\"cmd\":\"shutdown\"}\n").await;
        let _ = self.stdin.flush().await;
        let _ = self.child.kill().await;
    }
}

/// Locates `sidecar-pose/` in dev workspace or bundled Tauri resource directory.
pub fn resolve_sidecar_dir(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("sidecar-pose"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("sidecar-pose"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("sidecar-pose"));
            candidates.push(dir.join("resources").join("sidecar-pose"));
        }
    }
    if let Some(res) = resource_dir {
        candidates.push(res.join("sidecar-pose"));
        candidates.push(res.join("_up_").join("sidecar-pose"));
    }

    candidates
        .into_iter()
        .find(|p| p.join("worker.py").is_file())
}
