import React from "react";
import { CheckCircle2, Cpu, Download, Loader2, X } from "lucide-react";
import type { ModelDownloadProgress, ModelInfo } from "../types/motion";

interface ModelSetupModalProps {
  open: boolean;
  models: ModelInfo[];
  downloading: boolean;
  progress: ModelDownloadProgress | null;
  onDownloadPackage: (pkg: "basic" | "anime_2d" | "full") => void;
  onClose: () => void;
}

export const ModelSetupModal: React.FC<ModelSetupModalProps> = ({
  open,
  models,
  downloading,
  progress,
  onDownloadPackage,
  onClose,
}) => {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="w-full max-w-2xl rounded-2xl bg-[#141824] border border-slate-700/90 shadow-2xl overflow-hidden">
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <Cpu className="w-5 h-5 text-cyan-400" />
            <div>
              <h2 className="text-sm font-semibold text-slate-100">
                Pengelola Bobot Model AI Lokal (100% Gratis & Offline)
              </h2>
              <p className="text-xs text-slate-400">
                Unduh sekali ke folder Vault lokal, lalu jalankan seluruh mode
                secara offline selamanya.
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            disabled={downloading}
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer disabled:opacity-40"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-5 space-y-4 max-h-[65vh] overflow-y-auto">
          {/* Quick Download Packages */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <button
              disabled={downloading}
              onClick={() => onDownloadPackage("basic")}
              className="p-3 rounded-xl bg-[#1a2030] hover:bg-[#22293d] border border-slate-700 text-left transition cursor-pointer disabled:opacity-50"
            >
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-emerald-300">
                  Paket Dasar
                </span>
                <Download className="w-3.5 h-3.5 text-emerald-400" />
              </div>
              <div className="text-[11px] text-slate-400">
                MediaPipe Body + Hands + 52 Face (~40 MB)
              </div>
            </button>

            <button
              disabled={downloading}
              onClick={() => onDownloadPackage("anime_2d")}
              className="p-3 rounded-xl bg-indigo-500/15 hover:bg-indigo-500/25 border border-indigo-500/50 text-left transition cursor-pointer disabled:opacity-50"
            >
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-indigo-300">
                  Dasar + Anime 2D
                </span>
                <Download className="w-3.5 h-3.5 text-indigo-400" />
              </div>
              <div className="text-[11px] text-slate-300">
                Tambah DWPose 133 & YOLOX-L (~375 MB)
              </div>
            </button>

            <button
              disabled={downloading}
              onClick={() => onDownloadPackage("full")}
              className="p-3 rounded-xl bg-cyan-500/15 hover:bg-cyan-500/25 border border-cyan-500/50 text-left transition cursor-pointer disabled:opacity-50"
            >
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-cyan-300">
                  Lengkap (Semua)
                </span>
                <Download className="w-3.5 h-3.5 text-cyan-400" />
              </div>
              <div className="text-[11px] text-slate-300">
                Fast + Anime 2D + Studio CUDA (~727 MB)
              </div>
            </button>
          </div>

          {/* Live Download Progress */}
          {downloading && progress && (
            <div className="rounded-xl bg-indigo-950/50 border border-indigo-500/40 p-3.5 space-y-1.5">
              <div className="flex items-center justify-between text-xs">
                <span className="flex items-center gap-2 text-indigo-200 font-medium">
                  <Loader2 className="w-3.5 h-3.5 animate-spin text-cyan-400" />
                  Mengunduh {progress.model_name}...
                </span>
                <span className="font-mono text-cyan-300">
                  {progress.percent.toFixed(1)}%
                </span>
              </div>
              <div className="h-2 rounded-full bg-slate-800 overflow-hidden">
                <div
                  className="h-full bg-gradient-to-r from-indigo-500 to-cyan-400 transition-all duration-150"
                  style={{ width: `${progress.percent}%` }}
                />
              </div>
            </div>
          )}

          {/* Individual Model Status List */}
          <div className="space-y-2">
            {models.map((m) => (
              <div
                key={m.id}
                className="flex items-center justify-between gap-3 rounded-lg bg-[#10141f] border border-slate-800 px-3.5 py-2.5"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-medium text-slate-100">
                      {m.name}
                    </span>
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 uppercase">
                      {m.package}
                    </span>
                  </div>
                  <div className="text-[11px] text-slate-500 mt-0.5">
                    {m.description}
                  </div>
                </div>
                <div className="shrink-0 flex items-center gap-2">
                  <span className="text-[11px] font-mono text-slate-400">
                    {m.size_mb.toFixed(0)} MB
                  </span>
                  {m.installed ? (
                    <span className="inline-flex items-center gap-1 text-xs text-emerald-400 font-medium">
                      <CheckCircle2 className="w-4 h-4" />
                      Siap
                    </span>
                  ) : (
                    <span className="text-xs text-amber-400">Belum</span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="px-5 py-3 border-t border-slate-800 bg-[#10131d] flex justify-end">
          <button
            onClick={onClose}
            disabled={downloading}
            className="px-4 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs font-medium text-slate-200 transition cursor-pointer disabled:opacity-40"
          >
            Tutup
          </button>
        </div>
      </div>
    </div>
  );
};
