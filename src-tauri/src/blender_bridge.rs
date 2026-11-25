use crate::models::{BlenderSendOptions, BlenderStatus, MotionClip};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct PingResponse {
    ok: bool,
    blender_version: Option<String>,
    selected_armature: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImportPerformancePayload {
    clip_name: String,
    bvh_path: Option<String>,
    face_path: Option<String>,
    muscle_path: Option<String>,
    retarget_preset: String,
    global_scale: f64,
    stepped_anime: bool,
    apply_muscle_drivers: bool,
}

#[derive(Debug, Deserialize)]
struct ImportResponse {
    ok: bool,
    message: Option<String>,
    error: Option<String>,
}

pub async fn ping_blender(port: u16) -> BlenderStatus {
    let url = format!("http://127.0.0.1:{}/ping", port);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(1800))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return BlenderStatus {
                connected: false,
                blender_version: None,
                selected_armature: None,
                error: Some(e.to_string()),
            };
        }
    };

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<PingResponse>().await {
            Ok(data) if data.ok => BlenderStatus {
                connected: true,
                blender_version: data.blender_version,
                selected_armature: data.selected_armature,
                error: None,
            },
            Ok(_) => BlenderStatus {
                connected: false,
                blender_version: None,
                selected_armature: None,
                error: Some("Blender bridge returned ok=false".into()),
            },
            Err(e) => BlenderStatus {
                connected: false,
                blender_version: None,
                selected_armature: None,
                error: Some(format!("Invalid ping response: {}", e)),
            },
        },
        Ok(resp) => BlenderStatus {
            connected: false,
            blender_version: None,
            selected_armature: None,
            error: Some(format!("HTTP {}", resp.status().as_u16())),
        },
        Err(e) => BlenderStatus {
            connected: false,
            blender_version: None,
            selected_armature: None,
            error: Some(e.to_string()),
        },
    }
}

pub async fn send_clip_to_blender(
    port: u16,
    clip: &MotionClip,
    options: &BlenderSendOptions,
) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{}/import_performance", port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = ImportPerformancePayload {
        clip_name: clip.title.clone(),
        bvh_path: clip.bvh_path.clone(),
        face_path: clip.face_path.clone(),
        muscle_path: clip.muscle_path.clone(),
        retarget_preset: options.retarget_preset.clone(),
        global_scale: options.global_scale,
        stepped_anime: options.stepped_anime,
        apply_muscle_drivers: options.apply_muscle_drivers,
    };

    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Gagal menghubungi Blender di port {}: {}", port, e))?;

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Blender Bridge HTTP {}: {}", code, body));
    }

    let parsed: ImportResponse = resp
        .json()
        .await
        .map_err(|e| format!("Respons Blender tidak valid: {}", e))?;

    if parsed.ok {
        Ok(parsed
            .message
            .unwrap_or_else(|| format!("Berhasil mengirim '{}' ke Blender", clip.title)))
    } else {
        Err(parsed
            .error
            .unwrap_or_else(|| "Blender menolak impor performa".into()))
    }
}
