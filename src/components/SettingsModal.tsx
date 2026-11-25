import React, { useState, useEffect } from "react";
import { FolderOpen, Save, Wrench, X } from "lucide-react";
import type { AiMode, AppSettings } from "../types/motion";
import { api } from "../lib/api";

interface SettingsModalProps {
  open: boolean;
  settings: AppSettings | null;
  onSave: (settings: AppSettings) => Promise<void>;
  onCopyBlenderAddon: () => Promise<void>;
  onClose: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  open,
  settings,
  onSave,
  onCopyBlenderAddon,
  onClose,
}) => {
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(settings);
  }, [settings, open]);

  if (!open || !draft) return null;

  const handleBrowseVault = async () => {
    const dir = await api.pickFolder("Pilih Direktori Motion Vault");
    if (dir) {
      setDraft({ ...draft, vault_dir: dir });
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave(draft);
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="w-full max-w-lg rounded-2xl bg-[#141824] border border-slate-700/90 shadow-2xl overflow-hidden">
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
          <h2 className="text-sm font-semibold text-slate-100">
            Setelan MotionHarvest Studio
          </h2>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-5 space-y-4">
          {/* Vault Path */}
          <div>
            <label className="block text-xs text-slate-300 mb-1">
              Lokasi Motion Vault (SQLite, BVH, Face, Muscle, Models)
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={draft.vault_dir}
                onChange={(e) =>
                  setDraft({ ...draft, vault_dir: e.target.value })
                }
                className="flex-1 rounded-lg bg-[#0e1118] border border-slate-700 px-3 py-1.5 text-xs font-mono text-slate-200"
              />
              <button
                type="button"
                onClick={handleBrowseVault}
                className="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 border border-slate-700 text-xs text-slate-200 flex items-center gap-1.5 cursor-pointer"
              >
                <FolderOpen className="w-3.5 h-3.5 text-cyan-400" />
                Pilih
              </button>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-300 mb-1">
                Port Bridge Blender
              </label>
              <input
                type="number"
                value={draft.blender_port}
                onChange={(e) =>
                  setDraft({ ...draft, blender_port: Number(e.target.value) })
                }
                className="w-full rounded-lg bg-[#0e1118] border border-slate-700 px-3 py-1.5 text-xs font-mono text-slate-200"
              />
            </div>

            <div>
              <label className="block text-xs text-slate-300 mb-1">
                Default Mode AI
              </label>
              <select
                value={draft.default_ai_mode}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    default_ai_mode: e.target.value as AiMode,
                  })
                }
                className="w-full rounded-lg bg-[#0e1118] border border-slate-700 px-2.5 py-1.5 text-xs text-slate-200"
              >
                <option value="fast">Fast CPU/iGPU</option>
                <option value="studio_cuda">Studio GPU CUDA</option>
                <option value="anime_2d">Anime 2D</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-300 mb-1">
                Default Scene-Cut Threshold
              </label>
              <input
                type="number"
                min={4}
                max={30}
                value={draft.scene_threshold}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    scene_threshold: Number(e.target.value),
                  })
                }
                className="w-full rounded-lg bg-[#0e1118] border border-slate-700 px-3 py-1.5 text-xs font-mono text-slate-200"
              />
            </div>

            <div>
              <label className="block text-xs text-slate-300 mb-1">
                Hardware Encode (NVENC)
              </label>
              <select
                value={draft.use_nvenc}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    use_nvenc: e.target.value as "auto" | "on" | "off",
                  })
                }
                className="w-full rounded-lg bg-[#0e1118] border border-slate-700 px-2.5 py-1.5 text-xs text-slate-200"
              >
                <option value="auto">Otomatis (Deteksi NVIDIA)</option>
                <option value="on">Selalu Aktif (h264_nvenc)</option>
                <option value="off">Nonaktif (CPU libx264)</option>
              </select>
            </div>
          </div>

          {/* One-click Blender Add-on installer / copier */}
          <div className="rounded-xl bg-[#0e1118] border border-slate-800 p-3.5 flex items-center justify-between gap-3">
            <div>
              <div className="text-xs font-medium text-slate-200">
                Add-on Blender 4.0+ Bridge
              </div>
              <div className="text-[11px] text-slate-500">
                Salin `motion_harvest_bridge.py` lalu instal lewat Edit →
                Preferences → Add-ons di Blender.
              </div>
            </div>
            <button
              type="button"
              onClick={onCopyBlenderAddon}
              className="shrink-0 flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600/25 hover:bg-indigo-600/40 border border-indigo-500/50 text-xs font-medium text-indigo-200 transition cursor-pointer"
            >
              <Wrench className="w-3.5 h-3.5" />
              Salin Add-on
            </button>
          </div>
        </div>

        <div className="px-5 py-3 border-t border-slate-800 bg-[#10131d] flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs font-medium text-slate-300 transition cursor-pointer"
          >
            Batal
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-xs font-medium text-white transition cursor-pointer disabled:opacity-50"
          >
            <Save className="w-3.5 h-3.5" />
            Simpan Setelan
          </button>
        </div>
      </div>
    </div>
  );
};
