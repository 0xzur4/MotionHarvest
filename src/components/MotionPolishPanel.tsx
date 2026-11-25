import React from "react";
import {
  Download,
  Footprints,
  Hand,
  Loader2,
  Send,
  Sliders,
  Smile,
  Zap,
} from "lucide-react";
import type {
  BlenderStatus,
  MotionClip,
  PolishConfig,
  RetargetPreset,
  TimingMode,
} from "../types/motion";

interface MotionPolishPanelProps {
  clip: MotionClip | null;
  polish: PolishConfig;
  compileMs: number | null;
  compiling: boolean;
  retargetPreset: RetargetPreset;
  blenderStatus: BlenderStatus;
  sendingBlender: boolean;
  onPolishChange: (next: PolishConfig) => void;
  onRetargetChange: (preset: RetargetPreset) => void;
  onSendToBlender: () => void;
  onExportBundle: () => void;
}

const TIMING_OPTIONS: Array<{
  value: TimingMode;
  label: string;
  desc: string;
}> = [
  { value: "smooth", label: "Smooth Ori FPS", desc: "Gerakan penuh mulus" },
  { value: "anime_12", label: "Anime 12 FPS", desc: "On Twos — gaya anime TV" },
  { value: "ghibli_8", label: "Ghibli 8 FPS", desc: "On Threes — sinematik 2D" },
  {
    value: "original_hold",
    label: "Ori Hold-Frame",
    desc: "Tiru ritme asli anime sumber",
  },
];

export const MotionPolishPanel: React.FC<MotionPolishPanelProps> = ({
  clip,
  polish,
  compileMs,
  compiling,
  retargetPreset,
  blenderStatus,
  sendingBlender,
  onPolishChange,
  onRetargetChange,
  onSendToBlender,
  onExportBundle,
}) => {
  const disabled = !clip;

  return (
    <aside className="w-[280px] shrink-0 bg-[#11141c] border-l border-slate-800/80 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-slate-800/80 flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-1.5">
          <Sliders className="w-3.5 h-3.5 text-indigo-400" />
          3. Motion Polish
        </span>
        {compiling ? (
          <span className="text-[10px] text-cyan-400 font-mono flex items-center gap-1">
            <Loader2 className="w-3 h-3 animate-spin" />
            Rust...
          </span>
        ) : compileMs !== null ? (
          <span className="text-[10px] text-emerald-400 font-mono">
            {compileMs} ms
          </span>
        ) : null}
      </div>

      {/* Polish Controls */}
      <div className="flex-1 overflow-y-auto p-3.5 space-y-4">
        {/* Smoothing Sliders */}
        <div className="space-y-3">
          <div>
            <div className="flex items-center justify-between text-xs mb-1">
              <span className="text-slate-300">Body Smoothing</span>
              <span className="font-mono text-indigo-300">
                {Math.round(polish.body_smooth * 100)}%
              </span>
            </div>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              disabled={disabled}
              value={polish.body_smooth}
              onChange={(e) =>
                onPolishChange({
                  ...polish,
                  body_smooth: Number(e.target.value),
                })
              }
              className="w-full h-1.5 bg-slate-800 rounded cursor-pointer disabled:opacity-40"
            />
          </div>

          <div>
            <div className="flex items-center justify-between text-xs mb-1">
              <span className="text-slate-300">Face Smoothing (52 ARKit)</span>
              <span className="font-mono text-cyan-300">
                {Math.round(polish.face_smooth * 100)}%
              </span>
            </div>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              disabled={disabled}
              value={polish.face_smooth}
              onChange={(e) =>
                onPolishChange({
                  ...polish,
                  face_smooth: Number(e.target.value),
                })
              }
              className="w-full h-1.5 bg-slate-800 rounded cursor-pointer disabled:opacity-40"
            />
          </div>
        </div>

        {/* Foot Lock Toggle */}
        <div className="flex items-center justify-between rounded-lg bg-[#171b26] border border-slate-800 px-3 py-2">
          <div className="flex items-center gap-2">
            <Footprints className="w-4 h-4 text-emerald-400" />
            <div>
              <div className="text-xs font-medium text-slate-200">
                Foot Lock
              </div>
              <div className="text-[10px] text-slate-500">
                Kunci Heel + Toe ke lantai
              </div>
            </div>
          </div>
          <button
            type="button"
            disabled={disabled}
            onClick={() =>
              onPolishChange({ ...polish, foot_lock: !polish.foot_lock })
            }
            className={`px-2.5 py-0.5 rounded text-[10px] font-semibold border transition cursor-pointer disabled:opacity-40 ${
              polish.foot_lock
                ? "bg-emerald-500/20 border-emerald-500/60 text-emerald-300"
                : "bg-slate-800 border-slate-700 text-slate-400"
            }`}
          >
            {polish.foot_lock ? "ON" : "OFF"}
          </button>
        </div>

        {/* Data Channel Checkboxes */}
        <div className="space-y-2">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
            Include Kanal Data
          </div>

          <label className="flex items-center justify-between rounded-lg bg-[#171b26] border border-slate-800 px-3 py-2 cursor-pointer">
            <span className="flex items-center gap-2 text-xs text-slate-200">
              <Hand className="w-3.5 h-3.5 text-pink-400" />
              Jari Tangan (+30 Tulang)
            </span>
            <input
              type="checkbox"
              disabled={disabled}
              checked={polish.include_hands}
              onChange={(e) =>
                onPolishChange({ ...polish, include_hands: e.target.checked })
              }
              className="accent-indigo-500 w-3.5 h-3.5"
            />
          </label>

          <label className="flex items-center justify-between rounded-lg bg-[#171b26] border border-slate-800 px-3 py-2 cursor-pointer">
            <span className="flex items-center gap-2 text-xs text-slate-200">
              <Smile className="w-3.5 h-3.5 text-cyan-400" />
              52 Ekspresi Wajah (.face.json)
            </span>
            <input
              type="checkbox"
              disabled={disabled}
              checked={polish.include_face}
              onChange={(e) =>
                onPolishChange({ ...polish, include_face: e.target.checked })
              }
              className="accent-indigo-500 w-3.5 h-3.5"
            />
          </label>

          <label className="flex items-center justify-between rounded-lg bg-[#171b26] border border-slate-800 px-3 py-2 cursor-pointer">
            <span className="flex items-center gap-2 text-xs text-slate-200">
              <Zap className="w-3.5 h-3.5 text-amber-400" />
              Muscle Drivers (.muscle.json)
            </span>
            <input
              type="checkbox"
              disabled={disabled}
              checked={polish.include_muscle}
              onChange={(e) =>
                onPolishChange({ ...polish, include_muscle: e.target.checked })
              }
              className="accent-indigo-500 w-3.5 h-3.5"
            />
          </label>
        </div>

        {/* Timing Preset Radio Group */}
        <div className="space-y-1.5">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
            Timing Preset (Body + Face)
          </div>
          <div className="space-y-1.5">
            {TIMING_OPTIONS.map((opt) => {
              const active = polish.timing_mode === opt.value;
              return (
                <button
                  key={opt.value}
                  type="button"
                  disabled={disabled}
                  onClick={() =>
                    onPolishChange({ ...polish, timing_mode: opt.value })
                  }
                  className={`w-full text-left rounded-lg border px-2.5 py-1.5 transition cursor-pointer disabled:opacity-40 ${
                    active
                      ? "bg-indigo-600/20 border-indigo-500 text-indigo-100"
                      : "bg-[#171b26] border-slate-800 text-slate-300 hover:border-slate-700"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-medium">{opt.label}</span>
                    <span
                      className={`w-2.5 h-2.5 rounded-full border ${
                        active
                          ? "bg-indigo-400 border-indigo-300"
                          : "border-slate-600"
                      }`}
                    />
                  </div>
                  <div className="text-[10px] text-slate-500">{opt.desc}</div>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Retarget & Export Actions */}
      <div className="border-t border-slate-800/80 bg-[#0e1118] p-3.5 space-y-2.5">
        <div>
          <label className="block text-[11px] text-slate-400 mb-1">
            Retarget Target Rig (Blender)
          </label>
          <select
            value={retargetPreset}
            onChange={(e) => onRetargetChange(e.target.value as RetargetPreset)}
            className="w-full rounded-md bg-[#171b26] border border-slate-700/80 px-2.5 py-1.5 text-xs text-slate-100"
          >
            <option value="raw">Raw BVH (21 / 51 Tulang)</option>
            <option value="vroid">VRoid / Anime 3D (VRM)</option>
            <option value="rigify">Blender Rigify</option>
            <option value="mixamo">Mixamo Standard</option>
          </select>
        </div>

        <button
          disabled={disabled || sendingBlender}
          onClick={onSendToBlender}
          className={`w-full flex items-center justify-center gap-2 py-2 rounded-lg text-xs font-semibold transition cursor-pointer disabled:opacity-40 ${
            blenderStatus.connected
              ? "bg-gradient-to-r from-indigo-600 to-cyan-600 hover:from-indigo-500 hover:to-cyan-500 text-white shadow-md shadow-indigo-500/20"
              : "bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-200"
          }`}
        >
          {sendingBlender ? (
            <Loader2 className="w-3.5 h-3.5 animate-spin" />
          ) : (
            <Send className="w-3.5 h-3.5" />
          )}
          Kirim ke Blender (1-Klik)
        </button>

        <button
          disabled={disabled}
          onClick={onExportBundle}
          className="w-full flex items-center justify-center gap-2 py-1.5 rounded-lg bg-[#171b26] hover:bg-slate-800 border border-slate-700/80 text-xs font-medium text-slate-200 transition cursor-pointer disabled:opacity-40"
        >
          <Download className="w-3.5 h-3.5 text-cyan-400" />
          Export Bundle (.bvh + .face + .muscle)
        </button>
      </div>
    </aside>
  );
};
