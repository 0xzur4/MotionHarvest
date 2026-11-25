import React from "react";
import {
  Activity,
  Cpu,
  Download,
  Film,
  FolderPlus,
  Settings,
} from "lucide-react";
import type { BlenderStatus, ModelInfo } from "../types/motion";

interface TopBarProps {
  blenderStatus: BlenderStatus;
  models: ModelInfo[];
  onAddVideos: () => void;
  onAddFolder: () => void;
  onOpenModels: () => void;
  onOpenSettings: () => void;
}

export const TopBar: React.FC<TopBarProps> = ({
  blenderStatus,
  models,
  onAddVideos,
  onAddFolder,
  onOpenModels,
  onOpenSettings,
}) => {
  const basicReady = models
    .filter((m) => m.package === "basic")
    .every((m) => m.installed);
  const installedCount = models.filter((m) => m.installed).length;

  return (
    <header className="h-12 shrink-0 bg-[#12151e] border-b border-slate-800/80 px-4 flex items-center justify-between select-none">
      {/* Brand + Source Buttons */}
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2 pr-3 border-r border-slate-800">
          <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-indigo-500 to-cyan-400 flex items-center justify-center shadow-md shadow-indigo-500/20">
            <Activity className="w-4 h-4 text-slate-950 stroke-[2.5]" />
          </div>
          <div className="leading-tight">
            <div className="text-sm font-semibold tracking-tight text-slate-100">
              MotionHarvest Studio
            </div>
            <div className="text-[10px] text-indigo-400 font-medium">
              Free Full Edition v1.0
            </div>
          </div>
        </div>

        <button
          onClick={onAddVideos}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-xs font-medium text-white transition cursor-pointer shadow-sm"
        >
          <Film className="w-3.5 h-3.5" />
          + Video
        </button>
        <button
          onClick={onAddFolder}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-slate-800 hover:bg-slate-700 border border-slate-700/80 text-xs font-medium text-slate-200 transition cursor-pointer"
        >
          <FolderPlus className="w-3.5 h-3.5 text-cyan-400" />
          + Folder
        </button>
      </div>

      {/* Status & Settings */}
      <div className="flex items-center gap-2.5">
        <button
          onClick={onOpenModels}
          className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border transition cursor-pointer ${
            basicReady
              ? "bg-slate-800/80 border-slate-700 text-slate-300 hover:bg-slate-800"
              : "bg-amber-500/15 border-amber-500/40 text-amber-300 hover:bg-amber-500/25"
          }`}
          title="Kelola bobot AI lokal (MediaPipe, DWPose, RTMPose3D)"
        >
          {basicReady ? (
            <Cpu className="w-3.5 h-3.5 text-cyan-400" />
          ) : (
            <Download className="w-3.5 h-3.5 text-amber-400 animate-bounce" />
          )}
          <span>
            AI Models ({installedCount}/{models.length})
          </span>
        </button>

        <div
          className={`flex items-center gap-2 px-3 py-1 rounded-full text-xs font-medium border ${
            blenderStatus.connected
              ? "bg-emerald-500/10 border-emerald-500/40 text-emerald-300"
              : "bg-slate-800/70 border-slate-700/80 text-slate-400"
          }`}
          title={
            blenderStatus.connected
              ? `Blender ${blenderStatus.blender_version ?? ""} | Armature: ${
                  blenderStatus.selected_armature ?? "Belum dipilih"
                }`
              : "Aktifkan Add-on MotionHarvest Bridge di Blender (N -> MotionHarvest)"
          }
        >
          <span>
            Blender: {blenderStatus.connected ? "Terhubung" : "Offline"}
          </span>
          <span
            className={`w-2 h-2 rounded-full ${
              blenderStatus.connected
                ? "bg-emerald-400 shadow-[0_0_8px_#34d399]"
                : "bg-slate-500"
            }`}
          />
        </div>

        <button
          onClick={onOpenSettings}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md bg-slate-800 hover:bg-slate-700 border border-slate-700/80 text-xs font-medium text-slate-200 transition cursor-pointer"
        >
          <Settings className="w-3.5 h-3.5 text-slate-300" />
          Setelan
        </button>
      </div>
    </header>
  );
};
