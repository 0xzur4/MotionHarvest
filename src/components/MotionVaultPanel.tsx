import React, { useState } from "react";
import {
  CheckSquare,
  Download,
  FolderPlus,
  GitMerge,
  Hand,
  Search,
  Smile,
  Square,
  Star,
  Trash2,
} from "lucide-react";
import type { Collection, MotionClip, VaultFilter } from "../types/motion";

interface MotionVaultPanelProps {
  clips: MotionClip[];
  collections: Collection[];
  filter: VaultFilter;
  activeClipId: string | null;
  selectedIds: string[];
  onFilterChange: (next: VaultFilter) => void;
  onSelectClip: (clip: MotionClip) => void;
  onToggleMultiSelect: (clipId: string) => void;
  onClearMultiSelect: () => void;
  onToggleFavorite: (clipId: string) => void;
  onSendSelectedToBlend: () => void;
  onBatchExport: () => void;
  onBatchDelete: () => void;
  onCreateCollection: (name: string) => Promise<void>;
  onAssignCollection: (collectionId: string | null) => Promise<void>;
}

const CLIP_TYPES = [
  { id: "all", label: "All" },
  { id: "full_body", label: "Body" },
  { id: "face_only", label: "Face" },
  { id: "blended", label: "Blend" },
];

const BODY_TAGS = [
  { id: "walk", label: "Walk" },
  { id: "run", label: "Run" },
  { id: "jump", label: "Jump" },
  { id: "sit_crouch", label: "Sit" },
  { id: "gesture", label: "Gesture" },
  { id: "action", label: "Action" },
  { id: "idle", label: "Idle" },
];

const EMOTION_TAGS = [
  { id: "happy", label: "😊 Happy" },
  { id: "angry", label: "😠 Angry" },
  { id: "sad", label: "😢 Sad" },
  { id: "surprised", label: "😮 Surprised" },
  { id: "talking", label: "💬 Talk" },
];

export const MotionVaultPanel: React.FC<MotionVaultPanelProps> = ({
  clips,
  collections,
  filter,
  activeClipId,
  selectedIds,
  onFilterChange,
  onSelectClip,
  onToggleMultiSelect,
  onClearMultiSelect,
  onToggleFavorite,
  onSendSelectedToBlend,
  onBatchExport,
  onBatchDelete,
  onCreateCollection,
  onAssignCollection,
}) => {
  const [newColName, setNewColName] = useState("");
  const [showNewCol, setShowNewCol] = useState(false);

  const handleCreateCol = async () => {
    if (!newColName.trim()) return;
    await onCreateCollection(newColName.trim());
    setNewColName("");
    setShowNewCol(false);
  };

  return (
    <section className="h-[248px] shrink-0 bg-[#10131b] border-t border-slate-800/90 flex flex-col overflow-hidden">
      {/* Row 1: Vault Title + Search + Type Pills + Collections + Multi-Select Actions */}
      <div className="px-3.5 py-2 border-b border-slate-800/70 flex flex-wrap items-center gap-2">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-300 mr-1">
          4. Motion Vault ({clips.length})
        </span>

        {/* Instant Search */}
        <div className="relative w-48">
          <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            placeholder="Cari klip / tag..."
            value={filter.search ?? ""}
            onChange={(e) =>
              onFilterChange({ ...filter, search: e.target.value })
            }
            className="w-full pl-8 pr-2.5 py-1 rounded-md bg-[#171b26] border border-slate-700/80 text-xs text-slate-100 placeholder:text-slate-500 focus:outline-none focus:border-indigo-500"
          />
        </div>

        {/* Clip Type Tabs */}
        <div className="flex items-center bg-[#171b26] rounded-md p-0.5 border border-slate-800">
          {CLIP_TYPES.map((t) => {
            const active = (filter.clip_type ?? "all") === t.id;
            return (
              <button
                key={t.id}
                onClick={() => onFilterChange({ ...filter, clip_type: t.id })}
                className={`px-2 py-0.5 rounded text-[11px] font-medium transition cursor-pointer ${
                  active
                    ? "bg-indigo-600 text-white"
                    : "text-slate-400 hover:text-slate-200"
                }`}
              >
                {t.label}
              </button>
            );
          })}
        </div>

        {/* Collection Filter */}
        <select
          value={filter.collection_id ?? "all"}
          onChange={(e) =>
            onFilterChange({
              ...filter,
              collection_id: e.target.value === "all" ? undefined : e.target.value,
            })
          }
          className="rounded-md bg-[#171b26] border border-slate-700/80 px-2 py-1 text-[11px] text-slate-200"
        >
          <option value="all">Koleksi: Semua</option>
          {collections.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name} ({c.clip_count})
            </option>
          ))}
        </select>

        {showNewCol ? (
          <div className="flex items-center gap-1">
            <input
              type="text"
              placeholder="Nama koleksi"
              value={newColName}
              onChange={(e) => setNewColName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleCreateCol()}
              className="w-28 px-2 py-0.5 rounded bg-[#171b26] border border-indigo-500 text-xs text-slate-100"
              autoFocus
            />
            <button
              onClick={handleCreateCol}
              className="px-2 py-0.5 rounded bg-indigo-600 text-[11px] text-white cursor-pointer"
            >
              Simpan
            </button>
          </div>
        ) : (
          <button
            onClick={() => setShowNewCol(true)}
            className="p-1 rounded bg-[#171b26] hover:bg-slate-800 border border-slate-700 text-slate-300 cursor-pointer"
            title="Buat Koleksi Baru"
          >
            <FolderPlus className="w-3.5 h-3.5 text-cyan-400" />
          </button>
        )}

        {/* Favorites Toggle */}
        <button
          onClick={() =>
            onFilterChange({
              ...filter,
              favorites_only: !filter.favorites_only,
            })
          }
          className={`flex items-center gap-1 px-2 py-1 rounded-md border text-[11px] transition cursor-pointer ${
            filter.favorites_only
              ? "bg-amber-500/20 border-amber-500/60 text-amber-300"
              : "bg-[#171b26] border-slate-800 text-slate-400 hover:text-amber-300"
          }`}
        >
          <Star
            className={`w-3 h-3 ${filter.favorites_only ? "fill-amber-400" : ""}`}
          />
          Favorit
        </button>

        {/* Multi-Select Action Bar */}
        {selectedIds.length > 0 && (
          <div className="ml-auto flex items-center gap-1.5 bg-indigo-950/60 border border-indigo-500/40 rounded-lg px-2 py-0.5">
            <span className="text-[11px] font-semibold text-indigo-200">
              {selectedIds.length} dipilih
            </span>
            <button
              onClick={onSendSelectedToBlend}
              disabled={selectedIds.length < 2}
              className="flex items-center gap-1 px-2 py-0.5 rounded bg-cyan-600 hover:bg-cyan-500 text-[11px] font-medium text-white disabled:opacity-40 cursor-pointer"
            >
              <GitMerge className="w-3 h-3" />
              Gabung (Blend)
            </button>
            {collections.length > 0 && (
              <select
                defaultValue=""
                onChange={(e) => {
                  if (e.target.value) {
                    onAssignCollection(
                      e.target.value === "__none__" ? null : e.target.value
                    );
                    e.target.value = "";
                  }
                }}
                className="rounded bg-slate-900 border border-slate-700 px-1.5 py-0.5 text-[10px] text-slate-200"
              >
                <option value="" disabled>
                  + Koleksi
                </option>
                <option value="__none__">(Lepas Koleksi)</option>
                {collections.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            )}
            <button
              onClick={onBatchExport}
              className="flex items-center gap-1 px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 cursor-pointer"
            >
              <Download className="w-3 h-3" />
              Export
            </button>
            <button
              onClick={onBatchDelete}
              className="flex items-center gap-1 px-2 py-0.5 rounded bg-rose-900/60 hover:bg-rose-800 text-[11px] text-rose-200 cursor-pointer"
            >
              <Trash2 className="w-3 h-3" />
              Hapus
            </button>
            <button
              onClick={onClearMultiSelect}
              className="text-[10px] text-slate-400 hover:text-white px-1 cursor-pointer"
            >
              ✕
            </button>
          </div>
        )}
      </div>

      {/* Row 2: Body Tag & Emotion Tag Chips */}
      <div className="px-3.5 py-1.5 border-b border-slate-800/60 flex items-center gap-1.5 overflow-x-auto text-[10px]">
        <span className="text-slate-500 font-semibold mr-0.5 shrink-0">Tag:</span>
        {BODY_TAGS.map((t) => {
          const active = filter.body_tag === t.id;
          return (
            <button
              key={t.id}
              onClick={() =>
                onFilterChange({
                  ...filter,
                  body_tag: active ? undefined : t.id,
                })
              }
              className={`px-2 py-0.5 rounded-full border transition shrink-0 cursor-pointer ${
                active
                  ? "bg-indigo-500/25 border-indigo-400 text-indigo-200"
                  : "bg-[#161a24] border-slate-800 text-slate-400 hover:text-slate-200"
              }`}
            >
              {t.label}
            </button>
          );
        })}

        <span className="text-slate-700 mx-1">|</span>
        <span className="text-slate-500 font-semibold mr-0.5 shrink-0">
          Emosi:
        </span>
        {EMOTION_TAGS.map((e) => {
          const active = filter.emotion_tag === e.id;
          return (
            <button
              key={e.id}
              onClick={() =>
                onFilterChange({
                  ...filter,
                  emotion_tag: active ? undefined : e.id,
                })
              }
              className={`px-2 py-0.5 rounded-full border transition shrink-0 cursor-pointer ${
                active
                  ? "bg-pink-500/25 border-pink-400 text-pink-200"
                  : "bg-[#161a24] border-slate-800 text-slate-400 hover:text-slate-200"
              }`}
            >
              {e.label}
            </button>
          );
        })}
      </div>

      {/* Row 3: Clip Cards Grid */}
      <div className="flex-1 overflow-y-auto p-2.5">
        {clips.length === 0 ? (
          <div className="h-full flex items-center justify-center text-xs text-slate-500">
            Belum ada klip yang cocok dengan filter di Motion Vault.
          </div>
        ) : (
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-2">
            {clips.map((clip) => {
              const isActive = clip.id === activeClipId;
              const isChecked = selectedIds.includes(clip.id);
              const qPct = Math.round(clip.quality_score * 100);
              return (
                <div
                  key={clip.id}
                  onClick={() => onSelectClip(clip)}
                  className={`group rounded-lg border p-2.5 transition cursor-pointer flex flex-col justify-between gap-1.5 ${
                    isActive
                      ? "bg-indigo-600/15 border-indigo-500 shadow-md shadow-indigo-500/10"
                      : "bg-[#161a25] border-slate-800 hover:border-slate-600"
                  }`}
                >
                  <div className="flex items-start justify-between gap-1.5">
                    <div className="flex items-center gap-1.5 min-w-0">
                      <button
                        type="button"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          onToggleMultiSelect(clip.id);
                        }}
                        className="text-slate-500 hover:text-cyan-400 shrink-0 cursor-pointer"
                        title="Pilih untuk Blend / Batch Export"
                      >
                        {isChecked ? (
                          <CheckSquare className="w-3.5 h-3.5 text-cyan-400" />
                        ) : (
                          <Square className="w-3.5 h-3.5" />
                        )}
                      </button>
                      <span
                        className="text-xs font-medium text-slate-100 truncate"
                        title={clip.title}
                      >
                        {clip.title}
                      </span>
                    </div>

                    <button
                      type="button"
                      onClick={(ev) => {
                        ev.stopPropagation();
                        onToggleFavorite(clip.id);
                      }}
                      className="shrink-0 text-slate-500 hover:text-amber-400 cursor-pointer"
                    >
                      <Star
                        className={`w-3.5 h-3.5 ${
                          clip.is_favorite
                            ? "fill-amber-400 text-amber-400"
                            : ""
                        }`}
                      />
                    </button>
                  </div>

                  {/* Metadata Row */}
                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                    <span>
                      {clip.duration_sec.toFixed(1)}s · {clip.fps.toFixed(0)}fps
                    </span>
                    <div className="flex items-center gap-1">
                      {clip.has_hands && (
                        <span title="Jari Tangan (+30 tulang)">
                          <Hand className="w-3 h-3 text-pink-400" />
                        </span>
                      )}
                      {clip.has_face && (
                        <span title="52 Ekspresi Wajah ARKit">
                          <Smile className="w-3 h-3 text-cyan-400" />
                        </span>
                      )}
                      <span className="text-emerald-400">★{qPct}%</span>
                    </div>
                  </div>

                  {/* Tags Row */}
                  <div className="flex flex-wrap gap-1">
                    <span
                      className={`text-[9px] px-1.5 py-0.5 rounded uppercase font-semibold ${
                        clip.clip_type === "face_only"
                          ? "bg-cyan-500/20 text-cyan-300"
                          : clip.clip_type === "blended"
                          ? "bg-fuchsia-500/20 text-fuchsia-300"
                          : "bg-indigo-500/20 text-indigo-300"
                      }`}
                    >
                      {clip.clip_type.replace("_", " ")}
                    </span>
                    {clip.body_tags.slice(0, 2).map((t) => (
                      <span
                        key={t}
                        className="text-[9px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-300"
                      >
                        {t}
                      </span>
                    ))}
                    {clip.emotion_tags.slice(0, 2).map((t) => (
                      <span
                        key={t}
                        className="text-[9px] px-1.5 py-0.5 rounded bg-pink-500/15 text-pink-300"
                      >
                        {t}
                      </span>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
};
