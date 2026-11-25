import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AiMode,
  AppSettings,
  BatchJob,
  BlenderSendOptions,
  BlenderStatus,
  ClipPreviewData,
  Collection,
  ExportResult,
  HarvestConfig,
  ModelDownloadProgress,
  ModelInfo,
  MotionClip,
  PolishConfig,
  VaultFilter,
} from "../types/motion";

export const api = {
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),

  checkModels: () => invoke<ModelInfo[]>("check_models"),
  downloadModels: (pkg: string) =>
    invoke<ModelInfo[]>("download_models", { package: pkg }),

  enqueueSources: (paths: string[], aiMode: AiMode, config: HarvestConfig) =>
    invoke<BatchJob[]>("enqueue_sources", {
      paths,
      aiMode,
      config,
    }),

  listJobs: () => invoke<BatchJob[]>("list_jobs"),
  deleteJob: (jobId: string) => invoke<void>("delete_job", { jobId }),
  queueControl: (
    action: "start" | "pause" | "resume" | "stop" | "cancel",
    jobId?: string | null
  ) => invoke<BatchJob[]>("queue_control", { action, jobId: jobId ?? null }),

  queryVault: (filter: VaultFilter) =>
    invoke<MotionClip[]>("query_vault", { filter }),
  getClipPreview: (clipId: string) =>
    invoke<ClipPreviewData>("get_clip_preview", { clipId }),
  recompileClip: (clipId: string, polishConfig: PolishConfig) =>
    invoke<ClipPreviewData>("recompile_clip", { clipId, polishConfig }),

  blendClips: (clipIds: string[], transitionFrames: number, name: string) =>
    invoke<MotionClip>("blend_clips", {
      clipIds,
      transitionFrames,
      name,
    }),

  toggleFavorite: (clipId: string) =>
    invoke<MotionClip>("toggle_favorite", { clipId }),
  renameClip: (clipId: string, title: string) =>
    invoke<MotionClip>("rename_clip", { clipId, title }),
  deleteClips: (clipIds: string[]) =>
    invoke<number>("delete_clips", { clipIds }),

  manageCollections: (params: {
    action: "list" | "create" | "delete" | "assign";
    name?: string;
    collectionId?: string | null;
    clipIds?: string[];
  }) =>
    invoke<Collection[]>("manage_collections", {
      action: params.action,
      name: params.name ?? null,
      collectionId: params.collectionId ?? null,
      clipIds: params.clipIds ?? null,
    }),

  exportClips: (clipIds: string[], targetDir: string) =>
    invoke<ExportResult>("export_clips", { clipIds, targetDir }),

  pingBlender: () => invoke<BlenderStatus>("ping_blender"),
  sendToBlender: (clipId: string, options: BlenderSendOptions) =>
    invoke<string>("send_to_blender", { clipId, options }),

  copyBlenderAddon: (targetDir: string) =>
    invoke<string>("copy_blender_addon", { targetDir }),

  pickVideos: async (): Promise<string[]> => {
    const res = await open({
      multiple: true,
      directory: false,
      title: "Pilih Video Sumber (Film / Anime / Referensi)",
      filters: [
        {
          name: "Video Files",
          extensions: ["mp4", "mkv", "mov", "avi", "webm", "m4v"],
        },
      ],
    });
    if (!res) return [];
    return Array.isArray(res) ? res : [res];
  },

  pickFolder: async (title = "Pilih Folder"): Promise<string | null> => {
    const res = await open({
      multiple: false,
      directory: true,
      title,
    });
    if (!res || Array.isArray(res)) return null;
    return res;
  },

  onQueueUpdated: (cb: (jobs: BatchJob[]) => void): Promise<UnlistenFn> =>
    listen<BatchJob[]>("queue:updated", (e) => cb(e.payload)),

  onClipAdded: (cb: (clip: MotionClip) => void): Promise<UnlistenFn> =>
    listen<MotionClip>("vault:clip_added", (e) => cb(e.payload)),

  onModelProgress: (
    cb: (progress: ModelDownloadProgress) => void
  ): Promise<UnlistenFn> =>
    listen<ModelDownloadProgress>("model:download_progress", (e) =>
      cb(e.payload)
    ),
};
