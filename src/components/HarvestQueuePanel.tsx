import React from "react";
import {
  CheckCircle2,
  Film,
  Pause,
  Play,
  Square,
  Trash2,
  UploadCloud,
  Users,
  AlertCircle,
  Loader2,
} from "lucide-react";
import type { AiMode, BatchJob, HarvestConfig } from "../types/motion";

interface HarvestQueuePanelProps {
  jobs: BatchJob[];
  aiMode: AiMode;
  config: HarvestConfig;
  isDragOver: boolean;
  onAiModeChange: (mode: AiMode) => void;
  onConfigChange: (cfg: HarvestConfig) => void;
  onQueueControl: (action: "start" | "pause" | "resume" | "stop") => void;
  onDeleteJob: (jobId: string) => void;
  onAddVideos: () => void;
}

const STATUS_BADGE: Record<string, { label: string; cls: string }> = {
  queued: { label: "Antre", cls: "bg-slate-700 text-slate-300" },
  splitting: { label: "Scene Cut", cls: "bg-cyan-500/20 text-cyan-300" },
  processing: { label: "Panen AI", cls: "bg-indigo-500/20 text-indigo-300" },
  paused: { label: "Jeda", cls: "bg-amber-500/20 text-amber-300" },
  completed: { label: "Selesai", cls: "bg-emerald-500/20 text-emerald-300" },
  cancelled: { label: "Batal", cls: "bg-rose-500/20 text-rose-300" },
  failed: { label: "Gagal", cls: "bg-rose-500/20 text-rose-300" },
};

export const HarvestQueuePanel: React.FC<HarvestQueuePanelProps> = ({
  jobs,
  aiMode,
  config,
  isDragOver,
  onAiModeChange,
  onConfigChange,
  onQueueControl,
  onDeleteJob,
  onAddVideos,
}) => {
  const hasRunning = jobs.some(
    (j) => j.status === "processing" || j.status === "splitting"
  );

  return (
    <aside className="w-[290px] shrink-0 bg-[#11141c] border-r border-slate-800/80 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-slate-800/80 flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
          1. Harvest Queue
        </span>
        <span className="text-[11px] text-slate-500 font-mono">
          {jobs.length} job
        </span>
      </div>

      {/* Drag & Drop Zone */}
      <div className="p-3 border-b border-slate-800/70">
        <button
          type="button"
          onClick={onAddVideos}
          className={`w-full rounded-lg border border-dashed p-3 text-center transition cursor-pointer ${
            isDragOver
              ? "border-indigo-400 bg-indigo-500/15 text-indigo-200"
              : "border-slate-700/90 bg-slate-900/50 hover:border-indigo-500/70 hover:bg-slate-900 text-slate-400"
          }`}
        >
          <UploadCloud className="w-5 h-5 mx-auto mb-1 text-indigo-400" />
          <div className="text-xs font-medium text-slate-200">
            Seret Video / Folder ke Sini
          </div>
          <div className="text-[10px] text-slate-500 mt-0.5">
            MP4, MKV, MOV, AVI, WebM (10–120m / Batch)
          </div>
        </button>

        {/* Transport Queue Controls */}
        <div className="grid grid-cols-3 gap-1.5 mt-2.5">
          <button
            onClick={() => onQueueControl(hasRunning ? "resume" : "start")}
            className="flex items-center justify-center gap-1 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium transition cursor-pointer"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            Start
          </button>
          <button
            onClick={() => onQueueControl("pause")}
            className="flex items-center justify-center gap-1 py-1.5 rounded bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-200 text-xs font-medium transition cursor-pointer"
          >
            <Pause className="w-3.5 h-3.5" />
            Pause
          </button>
          <button
            onClick={() => onQueueControl("stop")}
            className="flex items-center justify-center gap-1 py-1.5 rounded bg-slate-800 hover:bg-rose-900/50 border border-slate-700 hover:border-rose-700/60 text-slate-300 hover:text-rose-200 text-xs font-medium transition cursor-pointer"
          >
            <Square className="w-3 h-3 fill-current" />
            Stop
          </button>
        </div>
      </div>

      {/* Job List */}
      <div className="flex-1 overflow-y-auto p-2.5 space-y-2">
        {jobs.length === 0 ? (
          <div className="text-center py-8 px-3 text-slate-500 text-xs">
            <Film className="w-7 h-7 mx-auto mb-2 opacity-30" />
            Belum ada antrean panen. Tambahkan video panjang, episode anime,
            atau folder klip.
          </div>
        ) : (
          jobs.map((job) => {
            const pct =
              job.total_segments > 0
                ? Math.round((job.done_segments / job.total_segments) * 100)
                : job.status === "completed"
                ? 100
                : 0;
            const badge = STATUS_BADGE[job.status] ?? STATUS_BADGE.queued;
            return (
              <div
                key={job.id}
                className="rounded-lg bg-[#171b26] border border-slate-800/90 p-2.5 space-y-1.5"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div
                      className="text-xs font-medium text-slate-100 truncate"
                      title={job.source_name}
                    >
                      {job.source_name}
                    </div>
                    <div className="flex items-center gap-1.5 mt-1">
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded font-medium inline-flex items-center gap-1 ${badge.cls}`}
                      >
                        {(job.status === "processing" ||
                          job.status === "splitting") && (
                          <Loader2 className="w-2.5 h-2.5 animate-spin" />
                        )}
                        {job.status === "completed" && (
                          <CheckCircle2 className="w-2.5 h-2.5" />
                        )}
                        {job.status === "failed" && (
                          <AlertCircle className="w-2.5 h-2.5" />
                        )}
                        {badge.label}
                      </span>
                      <span className="text-[10px] text-slate-500 uppercase">
                        {job.ai_mode.replace("_", " ")}
                      </span>
                    </div>
                  </div>
                  <button
                    onClick={() => onDeleteJob(job.id)}
                    className="p-1 text-slate-500 hover:text-rose-400 transition cursor-pointer"
                    title="Hapus job dari antrean"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>

                {/* Progress Bar */}
                <div className="space-y-1">
                  <div className="h-1.5 w-full rounded-full bg-slate-800 overflow-hidden">
                    <div
                      className={`h-full transition-all duration-300 ${
                        job.status === "failed"
                          ? "bg-rose-500"
                          : job.status === "completed"
                          ? "bg-emerald-500"
                          : "bg-gradient-to-r from-indigo-500 to-cyan-400"
                      }`}
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  <div className="flex items-center justify-between text-[10px] text-slate-400 font-mono">
                    <span>
                      {job.done_segments}/{job.total_segments} shots ({pct}%)
                    </span>
                    <span>
                      {job.face_segments} face · {job.skipped_segments} skip
                    </span>
                  </div>
                </div>

                {job.error_message && (
                  <div className="text-[10px] text-rose-400 bg-rose-500/10 border border-rose-500/20 rounded px-2 py-1 break-words">
                    {job.error_message}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Harvest Configuration Footer */}
      <div className="border-t border-slate-800/80 bg-[#0e1118] p-3 space-y-2.5">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
          Konfigurasi Panen
        </div>

        {/* AI Mode */}
        <div>
          <label className="block text-[11px] text-slate-400 mb-1">
            Mode AI Engine
          </label>
          <select
            value={aiMode}
            onChange={(e) => onAiModeChange(e.target.value as AiMode)}
            className="w-full rounded-md bg-[#171b26] border border-slate-700/80 px-2.5 py-1.5 text-xs text-slate-100 focus:outline-none focus:border-indigo-500"
          >
            <option value="fast">Fast CPU/iGPU (MediaPipe WholeBody)</option>
            <option value="studio_cuda">Studio GPU CUDA (RTMPose3D 133)</option>
            <option value="anime_2d">Anime 2D (DWPose + MotionBERT + Hold)</option>
          </select>
        </div>

        {/* Multi-Person Toggle */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-xs text-slate-300">
            <Users className="w-3.5 h-3.5 text-cyan-400" />
            <span>Multi-Orang (ByteTrack)</span>
          </div>
          <button
            type="button"
            onClick={() =>
              onConfigChange({ ...config, multi_person: !config.multi_person })
            }
            className={`px-2 py-0.5 rounded text-[10px] font-semibold border transition cursor-pointer ${
              config.multi_person
                ? "bg-indigo-600/30 border-indigo-500 text-indigo-200"
                : "bg-slate-800 border-slate-700 text-slate-400"
            }`}
          >
            {config.multi_person ? "ON" : "OFF"}
          </button>
        </div>

        {/* Scene Cut Sensitivity */}
        <div>
          <div className="flex items-center justify-between text-[11px] mb-1">
            <span className="text-slate-400">Scene-Cut Threshold</span>
            <span className="font-mono text-indigo-300">
              {config.scene_threshold.toFixed(0)}
            </span>
          </div>
          <input
            type="range"
            min={4}
            max={30}
            step={1}
            value={config.scene_threshold}
            onChange={(e) =>
              onConfigChange({
                ...config,
                scene_threshold: Number(e.target.value),
              })
            }
            className="w-full h-1.5 bg-slate-800 rounded-lg cursor-pointer"
          />
        </div>

        {/* Min / Max Clip Duration */}
        <div className="grid grid-cols-2 gap-2">
          <div>
            <label className="block text-[10px] text-slate-500 mb-0.5">
              Min Durasi (s)
            </label>
            <input
              type="number"
              min={0.5}
              max={10}
              step={0.2}
              value={config.min_clip_sec}
              onChange={(e) =>
                onConfigChange({
                  ...config,
                  min_clip_sec: Math.max(0.5, Number(e.target.value)),
                })
              }
              className="w-full rounded bg-[#171b26] border border-slate-700/80 px-2 py-1 text-xs font-mono text-slate-200"
            />
          </div>
          <div>
            <label className="block text-[10px] text-slate-500 mb-0.5">
              Max Durasi (s)
            </label>
            <input
              type="number"
              min={5}
              max={60}
              step={1}
              value={config.max_clip_sec}
              onChange={(e) =>
                onConfigChange({
                  ...config,
                  max_clip_sec: Math.max(5, Number(e.target.value)),
                })
              }
              className="w-full rounded bg-[#171b26] border border-slate-700/80 px-2 py-1 text-xs font-mono text-slate-200"
            />
          </div>
        </div>
      </div>
    </aside>
  );
};
