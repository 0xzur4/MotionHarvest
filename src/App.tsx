import React, { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Eye, GitMerge } from "lucide-react";
import { api } from "./lib/api";
import type {
  AiMode,
  AppSettings,
  BatchJob,
  BlenderStatus,
  ClipPreviewData,
  Collection,
  HarvestConfig,
  ModelDownloadProgress,
  ModelInfo,
  MotionClip,
  PolishConfig,
  RetargetPreset,
  VaultFilter,
} from "./types/motion";
import { TopBar } from "./components/TopBar";
import { HarvestQueuePanel } from "./components/HarvestQueuePanel";
import { Viewport3D } from "./components/Viewport3D";
import { MotionBlendTab } from "./components/MotionBlendTab";
import { MotionPolishPanel } from "./components/MotionPolishPanel";
import { MotionVaultPanel } from "./components/MotionVaultPanel";
import { ModelSetupModal } from "./components/ModelSetupModal";
import { SettingsModal } from "./components/SettingsModal";

const DEFAULT_POLISH: PolishConfig = {
  body_smooth: 0.35,
  face_smooth: 0.4,
  foot_lock: true,
  include_hands: true,
  include_face: true,
  include_muscle: true,
  timing_mode: "smooth",
  trim_start: 0,
  trim_end: 0,
};

const DEFAULT_HARVEST: HarvestConfig = {
  scene_threshold: 12,
  min_clip_sec: 1.2,
  max_clip_sec: 20,
  min_visibility: 0.5,
  multi_person: true,
  max_persons: 4,
};

export const App: React.FC = () => {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [modelModalOpen, setModelModalOpen] = useState(false);
  const [downloadingModels, setDownloadingModels] = useState(false);
  const [modelProgress, setModelProgress] =
    useState<ModelDownloadProgress | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const [blenderStatus, setBlenderStatus] = useState<BlenderStatus>({
    connected: false,
    blender_version: null,
    selected_armature: null,
    error: null,
  });

  // Queue state
  const [jobs, setJobs] = useState<BatchJob[]>([]);
  const [aiMode, setAiMode] = useState<AiMode>("fast");
  const [harvestConfig, setHarvestConfig] =
    useState<HarvestConfig>(DEFAULT_HARVEST);
  const [isDragOver, setIsDragOver] = useState(false);

  // Vault state
  const [clips, setClips] = useState<MotionClip[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [vaultFilter, setVaultFilter] = useState<VaultFilter>({
    clip_type: "all",
  });
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  // Center Workspace state
  const [centerTab, setCenterTab] = useState<"preview" | "blend">("preview");
  const [preview, setPreview] = useState<ClipPreviewData | null>(null);
  const [polish, setPolish] = useState<PolishConfig>(DEFAULT_POLISH);
  const [compileMs, setCompileMs] = useState<number | null>(null);
  const [compiling, setCompiling] = useState(false);
  const [retargetPreset, setRetargetPreset] = useState<RetargetPreset>("vroid");
  const [sendingBlender, setSendingBlender] = useState(false);
  const [blendSequence, setBlendSequence] = useState<MotionClip[]>([]);

  // Toast feedback
  const [toast, setToast] = useState<{
    text: string;
    tone: "info" | "success" | "error";
  } | null>(null);

  const showToast = useCallback(
    (text: string, tone: "info" | "success" | "error" = "info") => {
      setToast({ text, tone });
      window.setTimeout(() => {
        setToast((cur) => (cur?.text === text ? null : cur));
      }, 4200);
    },
    []
  );

  const refreshVault = useCallback(async (f: VaultFilter) => {
    try {
      const data = await api.queryVault(f);
      setClips(data);
    } catch (err) {
      console.error("queryVault error:", err);
    }
  }, []);

  // Keep refs fresh for long-lived Tauri event listeners (avoids stale closures)
  const vaultFilterRef = useRef(vaultFilter);
  const aiModeRef = useRef(aiMode);
  const harvestConfigRef = useRef(harvestConfig);
  useEffect(() => {
    vaultFilterRef.current = vaultFilter;
  }, [vaultFilter]);
  useEffect(() => {
    aiModeRef.current = aiMode;
  }, [aiMode]);
  useEffect(() => {
    harvestConfigRef.current = harvestConfig;
  }, [harvestConfig]);

  // Initial load & live Tauri event subscriptions
  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const [s, m, j, cols] = await Promise.all([
          api.getSettings(),
          api.checkModels(),
          api.listJobs(),
          api.manageCollections({ action: "list" }),
        ]);
        if (!mounted) return;
        setSettings(s);
        setAiMode(s.default_ai_mode);
        setHarvestConfig((prev) => ({
          ...prev,
          scene_threshold: s.scene_threshold,
        }));
        setModels(m);
        setJobs(j);
        setCollections(cols);

        // First-run Model Setup check
        const basicReady = m
          .filter((item) => item.package === "basic")
          .every((item) => item.installed);
        if (!basicReady) {
          setModelModalOpen(true);
        }
      } catch (err) {
        console.error("Init error:", err);
      }
    })();

    const unlisteners: Array<() => void> = [];
    api.onQueueUpdated((updatedJobs) => setJobs(updatedJobs)).then((u) => unlisteners.push(u));
    api.onClipAdded(() => {
      refreshVault(vaultFilterRef.current);
    }).then((u) => unlisteners.push(u));
    api.onModelProgress((p) => setModelProgress(p)).then((u) => unlisteners.push(u));

    // Native Tauri OS Drag-and-Drop handler
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setIsDragOver(true);
        } else if (event.payload.type === "drop") {
          setIsDragOver(false);
          const dropped = event.payload.paths;
          if (dropped && dropped.length > 0) {
            api
              .enqueueSources(dropped, aiModeRef.current, harvestConfigRef.current)
              .then((added) => {
                showToast(
                  `${added.length} video masuk antrean — klik Start untuk mulai panen`,
                  "success"
                );
              })
              .catch((e) => showToast(String(e), "error"));
          }
        } else {
          setIsDragOver(false);
        }
      })
      .then((u) => unlisteners.push(u))
      .catch(() => {});

    return () => {
      mounted = false;
      unlisteners.forEach((fn) => fn());
    };
  }, [refreshVault, showToast]);

  // Re-query vault whenever filter changes
  useEffect(() => {
    refreshVault(vaultFilter);
  }, [vaultFilter, refreshVault]);

  // Poll Blender Bridge every 4 seconds
  useEffect(() => {
    let active = true;
    const poll = async () => {
      try {
        const st = await api.pingBlender();
        if (active) setBlenderStatus(st);
      } catch {
        if (active) {
          setBlenderStatus({
            connected: false,
            blender_version: null,
            selected_armature: null,
            error: null,
          });
        }
      }
    };
    poll();
    const timer = window.setInterval(poll, 4000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  // Debounced 150ms pure-Rust recompile on Polish slider/toggle/trim changes
  const debounceRef = useRef<number | null>(null);
  const triggerRecompile = useCallback(
    (clipId: string, nextPolish: PolishConfig) => {
      if (debounceRef.current !== null) {
        window.clearTimeout(debounceRef.current);
      }
      debounceRef.current = window.setTimeout(async () => {
        setCompiling(true);
        const t0 = performance.now();
        try {
          const updated = await api.recompileClip(clipId, nextPolish);
          setCompileMs(Math.max(1, Math.round(performance.now() - t0)));
          setPreview(updated);
          setClips((prev) =>
            prev.map((c) => (c.id === updated.clip.id ? updated.clip : c))
          );
        } catch (err) {
          showToast(`Re-polish gagal: ${String(err)}`, "error");
        } finally {
          setCompiling(false);
        }
      }, 150);
    },
    [showToast]
  );

  const handleSelectClip = async (clip: MotionClip) => {
    setCenterTab("preview");
    setCompileMs(null);
    try {
      const data = await api.getClipPreview(clip.id);
      setPreview(data);
      setPolish(data.clip.polish_config);
    } catch (err) {
      showToast(`Gagal memuat preview: ${String(err)}`, "error");
    }
  };

  const handlePolishChange = (next: PolishConfig) => {
    setPolish(next);
    if (preview?.clip) {
      triggerRecompile(preview.clip.id, next);
    }
  };

  const handleTrimChange = (trimStart: number, trimEnd: number) => {
    const next = { ...polish, trim_start: trimStart, trim_end: trimEnd };
    handlePolishChange(next);
  };

  const handleAddVideos = async () => {
    const paths = await api.pickVideos();
    if (paths.length === 0) return;
    try {
      const added = await api.enqueueSources(paths, aiMode, harvestConfig);
      showToast(
        `${added.length} video masuk antrean — klik Start untuk mulai panen`,
        "success"
      );
    } catch (err) {
      showToast(String(err), "error");
    }
  };

  const handleAddFolder = async () => {
    const folder = await api.pickFolder("Pilih Folder Berisi Video");
    if (!folder) return;
    try {
      const added = await api.enqueueSources([folder], aiMode, harvestConfig);
      showToast(
        `${added.length} video dari folder masuk antrean — klik Start untuk mulai panen`,
        "success"
      );
    } catch (err) {
      showToast(String(err), "error");
    }
  };

  const handleQueueControl = async (
    action: "start" | "pause" | "resume" | "stop"
  ) => {
    try {
      const updated = await api.queueControl(action);
      setJobs(updated);
    } catch (err) {
      showToast(String(err), "error");
    }
  };

  const handleDownloadPackage = async (pkg: "basic" | "anime_2d" | "full") => {
    setDownloadingModels(true);
    setModelProgress(null);
    try {
      const refreshed = await api.downloadModels(pkg);
      setModels(refreshed);
      showToast("Model AI berhasil diunduh ke Vault lokal!", "success");
    } catch (err) {
      showToast(String(err), "error");
    } finally {
      setDownloadingModels(false);
    }
  };

  const handleSendToBlender = async () => {
    if (!preview?.clip) return;
    setSendingBlender(true);
    try {
      const msg = await api.sendToBlender(preview.clip.id, {
        retarget_preset: retargetPreset,
        global_scale: 1.0,
        stepped_anime: polish.timing_mode !== "smooth",
        apply_muscle_drivers: polish.include_muscle,
      });
      showToast(msg, "success");
    } catch (err) {
      showToast(String(err), "error");
    } finally {
      setSendingBlender(false);
    }
  };

  const handleExportClips = async (ids: string[]) => {
    if (ids.length === 0) return;
    const dir = await api.pickFolder("Pilih Folder Tujuan Export Bundle");
    if (!dir) return;
    try {
      const res = await api.exportClips(ids, dir);
      showToast(
        `Berhasil ekspor ${res.exported_clips} klip (${res.files_written.length} file) ke ${res.target_dir}`,
        "success"
      );
    } catch (err) {
      showToast(String(err), "error");
    }
  };

  const handleCopyBlenderAddon = async () => {
    const dir = await api.pickFolder(
      "Pilih Folder Tujuan Salin motion_harvest_bridge.py"
    );
    if (!dir) return;
    try {
      const outPath = await api.copyBlenderAddon(dir);
      showToast(`Add-on disalin ke: ${outPath}`, "success");
    } catch (err) {
      showToast(String(err), "error");
    }
  };

  return (
    <div className="h-screen w-screen flex flex-col bg-[#0d0f14] text-slate-100 overflow-hidden">
      {/* Top Studio Header */}
      <TopBar
        blenderStatus={blenderStatus}
        models={models}
        onAddVideos={handleAddVideos}
        onAddFolder={handleAddFolder}
        onOpenModels={() => setModelModalOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {/* Middle 3-Panel Studio Row */}
      <div className="flex-1 flex min-h-0 overflow-hidden">
        {/* 1. Harvest Queue (Left) */}
        <HarvestQueuePanel
          jobs={jobs}
          aiMode={aiMode}
          config={harvestConfig}
          isDragOver={isDragOver}
          onAiModeChange={setAiMode}
          onConfigChange={setHarvestConfig}
          onQueueControl={handleQueueControl}
          onDeleteJob={async (id) => {
            await api.deleteJob(id);
            setJobs((prev) => prev.filter((j) => j.id !== id));
          }}
          onAddVideos={handleAddVideos}
        />

        {/* 2. Center 3D Viewport / Motion Blender Tabs */}
        <main className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden">
          <div className="h-9 shrink-0 bg-[#121620] border-b border-slate-800/80 px-3 flex items-center gap-2">
            <button
              onClick={() => setCenterTab("preview")}
              className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition cursor-pointer ${
                centerTab === "preview"
                  ? "bg-indigo-600/25 text-indigo-200 border border-indigo-500/50"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <Eye className="w-3.5 h-3.5" />
              Preview Klip (3D & Wajah)
            </button>
            <button
              onClick={() => setCenterTab("blend")}
              className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition cursor-pointer ${
                centerTab === "blend"
                  ? "bg-cyan-600/25 text-cyan-200 border border-cyan-500/50"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <GitMerge className="w-3.5 h-3.5" />
              Blend Klip (SLERP Sequencer)
              {blendSequence.length > 0 && (
                <span className="px-1.5 rounded-full bg-cyan-500/30 text-[10px] font-mono">
                  {blendSequence.length}
                </span>
              )}
            </button>
          </div>

          {centerTab === "preview" ? (
            <Viewport3D
              preview={preview}
              polish={polish}
              onTrimChange={handleTrimChange}
            />
          ) : (
            <MotionBlendTab
              allClips={clips}
              sequence={blendSequence}
              onSequenceChange={setBlendSequence}
              onBlendSave={async (ids, tf, name) => {
                const created = await api.blendClips(ids, tf, name);
                showToast(`Klip gabungan '${created.title}' tersimpan!`, "success");
                await refreshVault(vaultFilter);
                await handleSelectClip(created);
              }}
            />
          )}
        </main>

        {/* 3. Motion Polish (Right) */}
        <MotionPolishPanel
          clip={preview?.clip ?? null}
          polish={polish}
          compileMs={compileMs}
          compiling={compiling}
          retargetPreset={retargetPreset}
          blenderStatus={blenderStatus}
          sendingBlender={sendingBlender}
          onPolishChange={handlePolishChange}
          onRetargetChange={setRetargetPreset}
          onSendToBlender={handleSendToBlender}
          onExportBundle={() =>
            preview?.clip && handleExportClips([preview.clip.id])
          }
        />
      </div>

      {/* 4. Motion Vault (Bottom) */}
      <MotionVaultPanel
        clips={clips}
        collections={collections}
        filter={vaultFilter}
        activeClipId={preview?.clip.id ?? null}
        selectedIds={selectedIds}
        onFilterChange={setVaultFilter}
        onSelectClip={handleSelectClip}
        onToggleMultiSelect={(id) =>
          setSelectedIds((prev) =>
            prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]
          )
        }
        onClearMultiSelect={() => setSelectedIds([])}
        onToggleFavorite={async (id) => {
          const updated = await api.toggleFavorite(id);
          setClips((prev) => prev.map((c) => (c.id === id ? updated : c)));
        }}
        onSendSelectedToBlend={() => {
          const chosen = clips.filter(
            (c) => selectedIds.includes(c.id) && c.clip_type !== "face_only"
          );
          if (chosen.length < 2) {
            showToast(
              "Pilih minimal 2 klip tubuh untuk digabungkan di Motion Blender.",
              "error"
            );
            return;
          }
          setBlendSequence(chosen.slice(0, 5));
          setCenterTab("blend");
        }}
        onBatchExport={() => handleExportClips(selectedIds)}
        onBatchDelete={async () => {
          const count = await api.deleteClips(selectedIds);
          if (preview?.clip && selectedIds.includes(preview.clip.id)) {
            setPreview(null);
          }
          setSelectedIds([]);
          await refreshVault(vaultFilter);
          showToast(`${count} klip dihapus dari Vault`, "info");
        }}
        onCreateCollection={async (name) => {
          const cols = await api.manageCollections({ action: "create", name });
          setCollections(cols);
          showToast(`Koleksi '${name}' dibuat`, "success");
        }}
        onAssignCollection={async (collectionId) => {
          const cols = await api.manageCollections({
            action: "assign",
            collectionId,
            clipIds: selectedIds,
          });
          setCollections(cols);
          await refreshVault(vaultFilter);
          showToast("Koleksi klip diperbarui", "success");
        }}
      />

      {/* Modals */}
      <ModelSetupModal
        open={modelModalOpen}
        models={models}
        downloading={downloadingModels}
        progress={modelProgress}
        onDownloadPackage={handleDownloadPackage}
        onClose={() => setModelModalOpen(false)}
      />

      <SettingsModal
        open={settingsOpen}
        settings={settings}
        onSave={async (next) => {
          const saved = await api.saveSettings(next);
          setSettings(saved);
          showToast("Setelan tersimpan", "success");
        }}
        onCopyBlenderAddon={handleCopyBlenderAddon}
        onClose={() => setSettingsOpen(false)}
      />

      {/* Toast Notification */}
      {toast && (
        <div
          className={`fixed bottom-5 right-5 z-50 px-4 py-2.5 rounded-xl border text-xs font-medium shadow-2xl backdrop-blur-md transition ${
            toast.tone === "success"
              ? "bg-emerald-950/90 border-emerald-500/60 text-emerald-200"
              : toast.tone === "error"
              ? "bg-rose-950/90 border-rose-500/60 text-rose-200"
              : "bg-slate-900/95 border-indigo-500/50 text-slate-100"
          }`}
        >
          {toast.text}
        </div>
      )}
    </div>
  );
};

export default App;
