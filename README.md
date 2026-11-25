# MotionHarvest Studio (Free Full Edition v1.0)

Aplikasi desktop *local-first* **100% gratis dan lengkap** untuk memanen performa akting karakter secara utuh dari video panjang (film, anime, referensi 10–120 menit) maupun folder berisi ratusan video pendek ke dalam **Motion Vault lokal** siap pakai di **Blender, Unreal Engine, Unity, dan Godot**.

---

## Fitur Utama

- **51 Tulang Standar (`.bvh`):** 21 tulang tubuh inti (termasuk engkel, **tumit / heel & ujung kaki / toe** dengan *foot-locking*) + 30 tulang jari tangan kiri & kanan (toggle 21/51 tulang non-destruktif).
- **52 Ekspresi Wajah Standar ARKit (`.face.json`):** Senyum, marah, sedih, terkejut, kedip, lirikan mata, dan bicara—siap langsung menggerakkan Shape Keys (termasuk alias VRoid/VRM).
- **Kurva Tenaga Sendi & Pemicu Otot (`.muscle.json`):** Sudut tekukan siku/lutut, angkatan bahu, kecepatan sudut, dan *impact spike* untuk *Corrective Shape Keys* dan simulasi *physics* di Blender.
- **3 Mode AI Engine Lokal:**
  - **Fast (CPU/iGPU):** MediaPipe Whole-Body (Pose 33 + Hands 21×2 + Face 52 ARKit).
  - **Studio GPU CUDA:** RTMW3D-X WholeBody 133 (ONNX CUDA) + 52 ARKit Face.
  - **Anime 2D Mode:** DWPose 133 + Temporal 2D→3D Lifter + Anime Face Mapper + detektor pola *hold-frame* asli anime (`cadence.py`).
- **Auto Scene-Cut & Pre-Filter 4 Jalur:**
  - Memotong video otomatis di setiap pergantian kamera (`FFmpeg scdet`, mendukung akselerasi `h264_nvenc`).
  - Memilah segmen menjadi `full_performance`, `multi_person` (YOLO/ByteTrack splitter per orang), `face_closeup` (dipanen ke katalog Face Only), atau `skipped_empty`.
- **Instant Motion Polish (`< 15 ms` di Rust):** Trim In/Out non-destruktif, *One-Euro Filter* terpisah (Body & Face), *Heel+Toe Foot Lock*, dan *Timing Preset* (`Smooth`, `Anime 12 FPS`, `Ghibli 8 FPS`, `Ori Hold-Frame`) tanpa menjalankan ulang AI.
- **Motion Sequencer & SLERP Blender:** Menggabungkan 2–5 klip secara mulus memakai SLERP Quaternion, penyelarasan posisi horizontal X/Z & arah hadap pinggul, serta lintas-pudar 52 ekspresi wajah.
- **Resumable Queue (SQLite WAL):** Aman *Pause / Stop* atau tutup aplikasi di tengah antrean panjang, lalu *Resume* tanpa duplikasi klip.
- **Blender 4.0+ Bridge 1-Klik (`127.0.0.1:9877`):** Impor BVH + Auto-Retarget 51 tulang (`Raw`, `Mixamo`, `Rigify`, `VRoid/VRM`) + 52 Shape Keys wajah + Muscle Driver Custom Properties + *Stepped Anime F-Curves*.

---

## Struktur Proyek

```text
motionharvest/
├── src/                                  # Frontend React + TypeScript + Tailwind v4 + Three.js
│   ├── App.tsx                           # Tata letak Dark Pro Studio 3-Panel + Bottom Vault
│   ├── lib/api.ts                        # Wrapper terketik Tauri IPC & event listener
│   ├── types/motion.ts                   # Kontrak tipe data TS <-> Rust
│   └── components/                       # TopBar, HarvestQueue, Viewport3D, FaceMonitor, BlendTab, Polish, Vault, Modals
├── src-tauri/                            # Backend Core Rust + Tauri v2
│   ├── src/db.rs                         # SQLite WAL, skema lengkap, filter Vault, crash recovery
│   ├── src/model_manager.rs              # Pengunduh bobot AI lokal gratis (progress streaming)
│   ├── src/blender_bridge.rs             # Client HTTP lokal ke Add-on Blender (:9877)
│   ├── src/pipeline/                     # splitter.rs (FFmpeg), sidecar.rs (NDJSON), queue.rs (tokio)
│   └── src/kinematics/                   # skeleton, one_euro, foot_lock, anime_timing, bvh_writer, muscle_curves, auto_tagger, motion_blender
├── sidecar-pose/                         # AI Sidecar Python 3.12 terisolasi (uv)
│   ├── worker.py                         # Warm worker NDJSON stdin/stdout
│   ├── prefilter.py                      # Smart Pre-Filter 4 Jalur
│   ├── tracker.py                        # Multi-person tracker (YOLO ByteTrack / HOG+IoU)
│   ├── cadence.py                        # Detektor hold-frame asli anime 2D
│   └── backends/                         # mediapipe_backend, cuda_studio_backend, anime_2d_backend
└── blender-addon/
    └── motion_harvest_bridge.py          # Add-on Blender 4.0+ (Panel N -> MotionHarvest)
```

---

## Cara Menggunakan (Windows `.exe`)

1. **Instalasi:** Jalankan installer hasil build di `src-tauri/target/release/bundle/nsis/MotionHarvest Studio_1.0.0_x64-setup.exe` (atau jalankan langsung `src-tauri/target/release/motionharvest-studio.exe`).
2. **Unduh Model AI (First-Run):** Saat pertama kali dibuka, pilih paket model di **AI Models**:
   - **Paket Dasar (~40 MB):** MediaPipe Body + Hands + 52 Face.
   - **Dasar + Anime 2D (~375 MB):** Tambah DWPose 133 & YOLOX-L.
   - **Lengkap (~727 MB):** Tambah RTMW3D-X WholeBody 133 Studio (CUDA).
3. **Panen Gerakan:**
   - Klik **+ Video** / **+ Folder** atau seret file ke **Harvest Queue**.
   - Atur **Mode AI**, toggle **Multi-Orang**, dan **Scene-Cut Threshold**.
   - Klik **Start** untuk memulai panen di latar belakang.
4. **Poles & Gabung Klip:**
   - Pilih klip di **Motion Vault** untuk melihat preview kerangka 3D & Face Monitor.
   - Sesuaikan *Smoothing*, *Foot Lock*, *Trim*, *Timing Preset*, atau buka tab **Blend Klip** untuk menggabungkan beberapa klip.
5. **Kirim ke Blender 1-Klik:**
   - Buka **⚙ Setelan → Salin Add-on** untuk menyalin `motion_harvest_bridge.py` (atau ambil langsung dari `blender-addon/motion_harvest_bridge.py`).
   - Di Blender 4.0+: `Edit → Preferences → Add-ons → Install from Disk...`, aktifkan **MotionHarvest Studio Bridge**.
   - Tekan `N` di 3D Viewport → tab **MotionHarvest** → pilih *Target Armature* & *Face Mesh* → klik **Start Bridge**.
   - Di MotionHarvest Studio, tunggu indikator `Blender: Terhubung ●` hijau, pilih preset retarget (`Raw` / `VRoid` / `Rigify` / `Mixamo`), lalu klik **Kirim ke Blender (1-Klik)**.

---

## Lokasi Data Motion Vault

Secara default seluruh data lokal disimpan di `Documents\MotionHarvestVault\`:

- `vault.db` — Database SQLite antrean, koleksi, dan indeks Motion Vault (siap di-query agen/MCP).
- `raw/` — Data performa mentah `*.rawperf.json` untuk re-polish instan non-destruktif.
- `bvh/` — File animasi tulang 21/51 sendi `*.bvh`.
- `face/` — File 52 ekspresi wajah ARKit `*.face.json`.
- `muscle/` — File kurva pemicu otot & *impact spike* `*.muscle.json`.
- `models/` — Bobot AI lokal offline.
- `sidecar-venv/` — Lingkungan Python terisolasi (otomatis dibuat oleh `uv` saat `.exe` dijalankan pertama kali).

---

## Panduan Development & Build

### Prasyarat
- **Rust** (`rustup`), **Node.js 20+ LTS**, **pnpm**, **Python 3.12**, **uv**, dan **FFmpeg / FFprobe** di `PATH`.

### Perintah Pengembangan
```powershell
# 1. Instal dependensi frontend
pnpm install

# 2. Siapkan lingkungan Python sidecar (CPU / standar)
uv sync --project sidecar-pose --python 3.12

# Opsional: aktifkan akselerasi NVIDIA CUDA + YOLO ByteTrack
uv sync --project sidecar-pose --python 3.12 --extra cuda

# 3. Jalankan aplikasi desktop mode dev
pnpm tauri dev
```

### Pengujian Otomatis
```powershell
# Unit test Rust (SQLite, FFmpeg Scene-Cut, BVH 21/51, One-Euro, Foot-Lock, Auto-Tagger, SLERP Blender, Queue Resume)
cargo test --manifest-path src-tauri/Cargo.toml

# Cek kompilasi TypeScript + Vite
pnpm build

# Tes sidecar Python (Anime cadence, 52 ARKit face mapper, 2D->3D lifter, 3 backends, NDJSON worker)
sidecar-pose\.venv\Scripts\python.exe sidecar-pose\test_sidecar.py
```

### Build Installer Rilis Windows (`.exe`)
```powershell
pnpm tauri build --bundles nsis
```
Output dihasilkan di:
- `src-tauri/target/release/bundle/nsis/MotionHarvest Studio_1.0.0_x64-setup.exe`
- `src-tauri/target/release/motionharvest-studio.exe`
