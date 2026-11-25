use crate::models::{ModelDownloadProgress, ModelInfo};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Download failed with status {0}")]
    BadStatus(u16),
}

struct ModelSpec {
    id: &'static str,
    name: &'static str,
    package: &'static str,
    filename: &'static str,
    url: &'static str,
    size_mb: f64,
    description: &'static str,
}

/// Official open-weight AI models used by MotionHarvest Studio (100% Free).
const MODEL_CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "mp_pose_heavy",
        name: "MediaPipe Pose Landmarker (Heavy 33-Joint)",
        package: "basic",
        filename: "pose_landmarker_heavy.task",
        url: "https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_heavy/float16/1/pose_landmarker_heavy.task",
        size_mb: 29.2,
        description: "Body + Heel + Toe 3D landmarks untuk Fast Mode (CPU/iGPU) & fallback.",
    },
    ModelSpec {
        id: "mp_hand",
        name: "MediaPipe Hand Landmarker (21x2 Jari)",
        package: "basic",
        filename: "hand_landmarker.task",
        url: "https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task",
        size_mb: 7.5,
        description: "Deteksi 30 sendi jari tangan kiri & kanan secara akurat.",
    },
    ModelSpec {
        id: "mp_face",
        name: "MediaPipe Face Landmarker (52 ARKit Blendshapes)",
        package: "basic",
        filename: "face_landmarker.task",
        url: "https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task",
        size_mb: 3.6,
        description: "52 ekspresi wajah ARKit (Senyum, Marah, Sedih, Kedip, Bicara).",
    },
    ModelSpec {
        id: "dwpose_wholebody",
        name: "DWPose / RTMPose-WholeBody 133 (ONNX)",
        package: "anime_2d",
        filename: "dw-ll_ucoco_384.onnx",
        url: "https://huggingface.co/yzd-v/DWPose/resolve/main/dw-ll_ucoco_384.onnx",
        size_mb: 128.2,
        description: "Detektor pose 2D 133 titik (tubuh+kaki+jari+wajah) tahan gaya garis Anime 2D.",
    },
    ModelSpec {
        id: "yolox_l_det",
        name: "YOLOX-L Person Detector (ONNX)",
        package: "anime_2d",
        filename: "yolox_l.onnx",
        url: "https://huggingface.co/yzd-v/DWPose/resolve/main/yolox_l.onnx",
        size_mb: 206.7,
        description: "Detektor subjek manusia & karakter anime multi-orang untuk DWPose/ByteTrack.",
    },
    ModelSpec {
        id: "rtmpose3d_wb",
        name: "RTMW3D-X WholeBody 133 Studio (ONNX CUDA)",
        package: "studio_cuda",
        filename: "rtmw3d-x_8xb64_cocktail14-384x288.onnx",
        url: "https://huggingface.co/Soykaf/RTMW3D-x/resolve/main/onnx/rtmw3d-x_8xb64_cocktail14-384x288-b0a0eab7_20240626.onnx",
        size_mb: 352.2,
        description: "Model 3D WholeBody 133 flagship tingkat studio (rtmlib) untuk GPU NVIDIA CUDA.",
    },
];

pub fn models_dir_for(vault_dir: &Path) -> PathBuf {
    vault_dir.join("models")
}

pub fn check_models(vault_dir: &Path) -> Vec<ModelInfo> {
    let mdir = models_dir_for(vault_dir);
    MODEL_CATALOG
        .iter()
        .map(|spec| {
            let full_path = mdir.join(spec.filename);
            let installed = full_path
                .metadata()
                .map(|m| m.is_file() && m.len() > 1024)
                .unwrap_or(false);
            ModelInfo {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                package: spec.package.to_string(),
                filename: spec.filename.to_string(),
                url: spec.url.to_string(),
                size_mb: spec.size_mb,
                installed,
                path: full_path.to_string_lossy().to_string(),
                description: spec.description.to_string(),
            }
        })
        .collect()
}

pub fn select_models_for_package(vault_dir: &Path, package_choice: &str) -> Vec<ModelInfo> {
    let all = check_models(vault_dir);
    all.into_iter()
        .filter(|m| match package_choice {
            "basic" => m.package == "basic",
            "anime_2d" => m.package == "basic" || m.package == "anime_2d",
            "full" | "all" => true,
            specific_id => m.id == specific_id || m.package == specific_id,
        })
        .collect()
}

pub async fn download_model<F>(
    vault_dir: &Path,
    model: &ModelInfo,
    mut on_progress: F,
) -> Result<PathBuf, ModelError>
where
    F: FnMut(ModelDownloadProgress),
{
    let mdir = models_dir_for(vault_dir);
    tokio::fs::create_dir_all(&mdir).await?;

    let target_path = mdir.join(&model.filename);
    if target_path
        .metadata()
        .map(|m| m.is_file() && m.len() > 1024)
        .unwrap_or(false)
    {
        on_progress(ModelDownloadProgress {
            model_id: model.id.clone(),
            model_name: model.name.clone(),
            downloaded_bytes: target_path.metadata()?.len(),
            total_bytes: target_path.metadata()?.len(),
            percent: 100.0,
            status: "completed".to_string(),
            error: None,
        });
        return Ok(target_path);
    }

    let tmp_path = mdir.join(format!("{}.part", model.filename));
    let client = reqwest::Client::builder()
        .user_agent("MotionHarvestStudio/1.0")
        .build()?;

    let resp = client.get(&model.url).send().await?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        on_progress(ModelDownloadProgress {
            model_id: model.id.clone(),
            model_name: model.name.clone(),
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 0.0,
            status: "failed".to_string(),
            error: Some(format!("HTTP {}", code)),
        });
        return Err(ModelError::BadStatus(code));
    }

    let total = resp.content_length().unwrap_or((model.size_mb * 1024.0 * 1024.0) as u64);
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emitted_pct: f64 = -1.0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let pct = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0).min(100.0)
        } else {
            0.0
        };
        if pct - last_emitted_pct >= 2.0 || downloaded == total {
            last_emitted_pct = pct;
            on_progress(ModelDownloadProgress {
                model_id: model.id.clone(),
                model_name: model.name.clone(),
                downloaded_bytes: downloaded,
                total_bytes: total,
                percent: pct,
                status: "downloading".to_string(),
                error: None,
            });
        }
    }

    file.flush().await?;
    drop(file);
    tokio::fs::rename(&tmp_path, &target_path).await?;

    on_progress(ModelDownloadProgress {
        model_id: model.id.clone(),
        model_name: model.name.clone(),
        downloaded_bytes: downloaded,
        total_bytes: downloaded,
        percent: 100.0,
        status: "completed".to_string(),
        error: None,
    });

    Ok(target_path)
}
