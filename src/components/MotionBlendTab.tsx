import React, { useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  GitMerge,
  Loader2,
  Plus,
  Trash2,
} from "lucide-react";
import type { MotionClip } from "../types/motion";

interface MotionBlendTabProps {
  allClips: MotionClip[];
  sequence: MotionClip[];
  onSequenceChange: (clips: MotionClip[]) => void;
  onBlendSave: (
    clipIds: string[],
    transitionFrames: number,
    name: string
  ) => Promise<void>;
}

export const MotionBlendTab: React.FC<MotionBlendTabProps> = ({
  allClips,
  sequence,
  onSequenceChange,
  onBlendSave,
}) => {
  const [transitionFrames, setTransitionFrames] = useState(10);
  const [blendName, setBlendName] = useState("");
  const [blending, setBlending] = useState(false);

  const bodyClips = allClips.filter((c) => c.clip_type !== "face_only");

  const addClip = (clip: MotionClip) => {
    if (sequence.length >= 5) return;
    onSequenceChange([...sequence, clip]);
  };

  const removeAt = (idx: number) => {
    onSequenceChange(sequence.filter((_, i) => i !== idx));
  };

  const move = (idx: number, dir: -1 | 1) => {
    const target = idx + dir;
    if (target < 0 || target >= sequence.length) return;
    const copy = [...sequence];
    const tmp = copy[idx];
    copy[idx] = copy[target];
    copy[target] = tmp;
    onSequenceChange(copy);
  };

  const totalFrames = sequence.reduce(
    (acc, c, idx) =>
      acc +
      c.frame_count -
      (idx > 0 ? Math.min(transitionFrames, Math.floor(c.frame_count / 2)) : 0),
    0
  );
  const estDur =
    sequence.length > 0 ? totalFrames / Math.max(1, sequence[0].fps) : 0;

  const handleSave = async () => {
    if (sequence.length < 2) return;
    setBlending(true);
    try {
      await onBlendSave(
        sequence.map((c) => c.id),
        transitionFrames,
        blendName
      );
      setBlendName("");
      onSequenceChange([]);
    } finally {
      setBlending(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-[#0f121a] p-5 overflow-y-auto space-y-5">
      <div>
        <h3 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
          <GitMerge className="w-4 h-4 text-cyan-400" />
          Motion Sequencer & SLERP Quaternion Blender
        </h3>
        <p className="text-xs text-slate-400 mt-1">
          Susun 2–5 klip secara berurutan (misal: Walk → Jump → Senyum). Rust
          menyelaraskan posisi X/Z & arah hadap pinggul lalu menyambung rotasi
          dengan SLERP Quaternion serta crossfade 52 ekspresi wajah tanpa
          teleportasi.
        </p>
      </div>

      {/* Active Sequence Track */}
      <div className="rounded-xl bg-[#141824] border border-slate-800 p-4 space-y-3">
        <div className="flex items-center justify-between text-xs">
          <span className="font-semibold text-slate-300">
            Urutan Klip ({sequence.length}/5)
          </span>
          <span className="font-mono text-cyan-300">
            Estimasi: {totalFrames} frames ({estDur.toFixed(2)}s)
          </span>
        </div>

        {sequence.length === 0 ? (
          <div className="rounded-lg border border-dashed border-slate-700 py-8 text-center text-xs text-slate-500">
            Tambahkan klip dari daftar di bawah atau pilih banyak klip di Motion
            Vault lalu klik &quot;Gabung (Blend)&quot;.
          </div>
        ) : (
          <div className="flex items-center gap-2 overflow-x-auto pb-1">
            {sequence.map((clip, idx) => (
              <React.Fragment key={`${clip.id}-${idx}`}>
                <div className="min-w-[175px] rounded-lg bg-[#1b2131] border border-indigo-500/40 p-2.5 space-y-1.5">
                  <div className="flex items-center justify-between gap-1">
                    <span className="text-[10px] font-mono text-indigo-300">
                      #{idx + 1}
                    </span>
                    <div className="flex items-center gap-0.5">
                      <button
                        onClick={() => move(idx, -1)}
                        disabled={idx === 0}
                        className="p-0.5 text-slate-400 hover:text-white disabled:opacity-30 cursor-pointer"
                      >
                        <ArrowLeft className="w-3 h-3" />
                      </button>
                      <button
                        onClick={() => move(idx, 1)}
                        disabled={idx === sequence.length - 1}
                        className="p-0.5 text-slate-400 hover:text-white disabled:opacity-30 cursor-pointer"
                      >
                        <ArrowRight className="w-3 h-3" />
                      </button>
                      <button
                        onClick={() => removeAt(idx)}
                        className="p-0.5 text-slate-400 hover:text-rose-400 cursor-pointer"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                  <div className="text-xs font-medium text-slate-100 truncate">
                    {clip.title}
                  </div>
                  <div className="text-[10px] font-mono text-slate-400">
                    {clip.duration_sec.toFixed(1)}s · {clip.frame_count}f
                  </div>
                </div>
                {idx < sequence.length - 1 && (
                  <div className="shrink-0 text-[10px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
                    ⇄ {transitionFrames}f
                  </div>
                )}
              </React.Fragment>
            ))}
          </div>
        )}

        {/* Controls */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-2 border-t border-slate-800">
          <div>
            <div className="flex items-center justify-between text-[11px] mb-1">
              <span className="text-slate-400">Transisi SLERP</span>
              <span className="font-mono text-indigo-300">
                {transitionFrames} frames
              </span>
            </div>
            <input
              type="range"
              min={2}
              max={30}
              value={transitionFrames}
              onChange={(e) => setTransitionFrames(Number(e.target.value))}
              className="w-full h-1.5 bg-slate-800 rounded cursor-pointer"
            />
          </div>

          <div>
            <label className="block text-[11px] text-slate-400 mb-1">
              Nama Klip Gabungan
            </label>
            <input
              type="text"
              placeholder="Misal: Walk_to_Jump_Smile"
              value={blendName}
              onChange={(e) => setBlendName(e.target.value)}
              className="w-full rounded-md bg-[#0e1118] border border-slate-700 px-2.5 py-1 text-xs text-slate-100"
            />
          </div>

          <div className="flex items-end">
            <button
              disabled={sequence.length < 2 || blending}
              onClick={handleSave}
              className="w-full flex items-center justify-center gap-1.5 py-1.5 rounded-md bg-gradient-to-r from-indigo-600 to-cyan-600 hover:from-indigo-500 hover:to-cyan-500 text-xs font-semibold text-white transition cursor-pointer disabled:opacity-40"
            >
              {blending ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <GitMerge className="w-3.5 h-3.5" />
              )}
              Simpan Klip Gabungan
            </button>
          </div>
        </div>
      </div>

      {/* Picker from Vault */}
      <div className="space-y-2">
        <div className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
          Tambah Klip dari Vault ({bodyClips.length} tersedia)
        </div>
        <div className="grid grid-cols-2 lg:grid-cols-3 gap-2">
          {bodyClips.map((clip) => (
            <button
              key={clip.id}
              disabled={sequence.length >= 5}
              onClick={() => addClip(clip)}
              className="flex items-center justify-between gap-2 rounded-lg bg-[#141824] hover:bg-[#1c2233] border border-slate-800 px-3 py-2 text-left transition cursor-pointer disabled:opacity-40"
            >
              <div className="min-w-0">
                <div className="text-xs font-medium text-slate-200 truncate">
                  {clip.title}
                </div>
                <div className="text-[10px] text-slate-500 font-mono">
                  {clip.duration_sec.toFixed(1)}s ·{" "}
                  {clip.body_tags.join(", ") || clip.clip_type}
                </div>
              </div>
              <Plus className="w-4 h-4 text-cyan-400 shrink-0" />
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};
