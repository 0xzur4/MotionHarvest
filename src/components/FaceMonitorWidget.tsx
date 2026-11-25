import React from "react";
import type { FaceTrackFile } from "../types/motion";

interface FaceMonitorWidgetProps {
  faceData: FaceTrackFile | null;
  currentFrame: number;
  expanded?: boolean;
}

const pct = (v: number) => Math.round(Math.max(0, Math.min(1, v)) * 100);

export const FaceMonitorWidget: React.FC<FaceMonitorWidgetProps> = ({
  faceData,
  currentFrame,
  expanded = false,
}) => {
  const frames = faceData?.frames ?? [];
  const safeIdx =
    frames.length > 0 ? Math.max(0, Math.min(frames.length - 1, currentFrame)) : 0;
  const bs: Record<string, number> = frames[safeIdx] ?? {};

  const get = (k: string) => bs[k] ?? 0;

  const smile = 0.5 * (get("mouthSmileLeft") + get("mouthSmileRight"));
  const frown = 0.5 * (get("mouthFrownLeft") + get("mouthFrownRight"));
  const angry = 0.5 * (get("browDownLeft") + get("browDownRight"));
  const browInnerUp = get("browInnerUp");
  const blinkL = get("eyeBlinkLeft");
  const blinkR = get("eyeBlinkRight");
  const wide = 0.5 * (get("eyeWideLeft") + get("eyeWideRight"));
  const jawOpen = get("jawOpen");
  const funnel = get("mouthFunnel");
  const gazeX =
    (get("eyeLookOutLeft") + get("eyeLookInRight") - get("eyeLookInLeft") - get("eyeLookOutRight")) *
    3.5;
  const gazeY =
    (get("eyeLookDownLeft") + get("eyeLookDownRight") - get("eyeLookUpLeft") - get("eyeLookUpRight")) *
    2.5;

  // Dynamic SVG facial geometry driven by live ARKit 52 blendshapes
  const eyeHeightL = Math.max(1.2, 8.5 * (1 - blinkL * 0.92) + wide * 3.5);
  const eyeHeightR = Math.max(1.2, 8.5 * (1 - blinkR * 0.92) + wide * 3.5);
  const browTiltAngry = angry * 8.0;
  const browSadLift = browInnerUp * 6.0;
  const mouthOpenH = 2.0 + jawOpen * 16.0;
  const mouthCornerShift = (frown - smile) * 9.0;
  const mouthHalfW = 16.0 - funnel * 6.0 + smile * 4.5;

  const meters = [
    { label: "Smile", emoji: "😊", val: smile, color: "bg-emerald-400" },
    { label: "Angry", emoji: "😠", val: angry, color: "bg-rose-400" },
    { label: "Sad", emoji: "😢", val: 0.5 * (frown + browInnerUp), color: "bg-sky-400" },
    { label: "Blink", emoji: "👁", val: 0.5 * (blinkL + blinkR), color: "bg-amber-400" },
    { label: "Jaw Open", emoji: "👄", val: jawOpen, color: "bg-indigo-400" },
    { label: "Eye Wide", emoji: "😮", val: wide, color: "bg-fuchsia-400" },
  ];

  // Top active raw ARKit channels in expanded mode
  const topActive = Object.entries(bs)
    .filter(([, v]) => v > 0.05)
    .sort((a, b) => b[1] - a[1])
    .slice(0, expanded ? 12 : 4);

  return (
    <div
      className={
        expanded
          ? "w-full h-full flex items-center justify-center p-6 bg-radial from-[#1b2030] to-[#0f121a]"
          : "w-48 rounded-xl bg-[#111520]/90 backdrop-blur-md border border-slate-700/80 p-2.5 shadow-xl"
      }
    >
      <div
        className={
          expanded
            ? "w-full max-w-2xl rounded-2xl bg-[#141824] border border-slate-700/80 p-6 grid grid-cols-1 md:grid-cols-2 gap-6 shadow-2xl"
            : "space-y-2"
        }
      >
        {/* Expressive Live Character Face SVG */}
        <div className="flex flex-col items-center">
          <div className="w-full flex items-center justify-between mb-1">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-cyan-400">
              Face 52 ARKit
            </span>
            <span className="text-[10px] font-mono text-slate-400">
              {frames.length > 0 ? `F${safeIdx + 1}` : "No Face"}
            </span>
          </div>

          <svg
            viewBox="0 0 120 120"
            className={expanded ? "w-56 h-56" : "w-24 h-24"}
          >
            {/* Head Silhouette */}
            <ellipse
              cx="60"
              cy="60"
              rx="42"
              ry="48"
              fill="#1d2436"
              stroke="#4f5d80"
              strokeWidth="2.5"
            />

            {/* Left Eyebrow */}
            <path
              d={`M 29 ${40 - browSadLift * 0.4} Q 40 ${34 - browSadLift} 51 ${
                40 + browTiltAngry - browSadLift
              }`}
              fill="none"
              stroke="#93c5fd"
              strokeWidth="3"
              strokeLinecap="round"
            />
            {/* Right Eyebrow */}
            <path
              d={`M 69 ${40 + browTiltAngry - browSadLift} Q 80 ${
                34 - browSadLift
              } 91 ${40 - browSadLift * 0.4}`}
              fill="none"
              stroke="#93c5fd"
              strokeWidth="3"
              strokeLinecap="round"
            />

            {/* Left Eye */}
            <ellipse
              cx="41"
              cy="54"
              rx="8.5"
              ry={eyeHeightL}
              fill="#0f172a"
              stroke="#60a5fa"
              strokeWidth="1.5"
            />
            {eyeHeightL > 2.5 && (
              <circle
                cx={41 + gazeX}
                cy={54 + gazeY}
                r={Math.min(3.5, eyeHeightL * 0.65)}
                fill="#38bdf8"
              />
            )}

            {/* Right Eye */}
            <ellipse
              cx="79"
              cy="54"
              rx="8.5"
              ry={eyeHeightR}
              fill="#0f172a"
              stroke="#60a5fa"
              strokeWidth="1.5"
            />
            {eyeHeightR > 2.5 && (
              <circle
                cx={79 + gazeX}
                cy={54 + gazeY}
                r={Math.min(3.5, eyeHeightR * 0.65)}
                fill="#38bdf8"
              />
            )}

            {/* Subtle Nose */}
            <path
              d="M 60 58 L 57 69 L 63 69"
              fill="none"
              stroke="#64748b"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />

            {/* Mouth */}
            <path
              d={`M ${60 - mouthHalfW} ${82 + mouthCornerShift}
                  Q 60 ${82 - mouthOpenH * 0.35} ${60 + mouthHalfW} ${
                82 + mouthCornerShift
              }
                  Q 60 ${82 + mouthOpenH} ${60 - mouthHalfW} ${
                82 + mouthCornerShift
              } Z`}
              fill={jawOpen > 0.12 ? "#881337" : "#334155"}
              stroke="#f472b6"
              strokeWidth="2"
              strokeLinejoin="round"
            />
          </svg>
        </div>

        {/* Real-time Blendshape Bars */}
        <div className="space-y-1.5 flex flex-col justify-center">
          {meters.map((m) => (
            <div key={m.label} className="space-y-0.5">
              <div className="flex items-center justify-between text-[10px]">
                <span className="text-slate-300">
                  {m.emoji} {m.label}
                </span>
                <span className="font-mono text-slate-400">{pct(m.val)}%</span>
              </div>
              <div className="h-1.5 rounded-full bg-slate-800 overflow-hidden">
                <div
                  className={`h-full ${m.color} transition-all duration-75`}
                  style={{ width: `${pct(m.val)}%` }}
                />
              </div>
            </div>
          ))}

          {expanded && topActive.length > 0 && (
            <div className="pt-3 border-t border-slate-800">
              <div className="text-[10px] uppercase tracking-wider text-slate-400 mb-1.5">
                Channel ARKit Aktif
              </div>
              <div className="flex flex-wrap gap-1">
                {topActive.map(([k, v]) => (
                  <span
                    key={k}
                    className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-cyan-300 border border-slate-700"
                  >
                    {k}: {pct(v)}%
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
