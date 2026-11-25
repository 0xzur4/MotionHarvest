import React, { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { BVHLoader } from "three/examples/jsm/loaders/BVHLoader.js";
import {
  Pause,
  Play,
  Repeat,
  RotateCcw,
  Scissors,
  SkipBack,
  SkipForward,
  Sparkles,
} from "lucide-react";
import type { ClipPreviewData, PolishConfig } from "../types/motion";
import { FaceMonitorWidget } from "./FaceMonitorWidget";

interface Viewport3DProps {
  preview: ClipPreviewData | null;
  polish: PolishConfig;
  onTrimChange: (trimStart: number, trimEnd: number) => void;
}

const SPEEDS = [0.25, 0.5, 1, 1.5, 2];

function fmtTime(sec: number): string {
  const s = Math.max(0, sec);
  const mins = Math.floor(s / 60);
  const rem = s % 60;
  return `${String(mins).padStart(2, "0")}:${rem.toFixed(1).padStart(4, "0")}`;
}

export const Viewport3D: React.FC<Viewport3DProps> = ({
  preview,
  polish,
  onTrimChange,
}) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<THREE.Scene | null>(null);
  const cameraRef = useRef<THREE.PerspectiveCamera | null>(null);
  const controlsRef = useRef<OrbitControls | null>(null);
  const mixerRef = useRef<THREE.AnimationMixer | null>(null);
  const actionRef = useRef<THREE.AnimationAction | null>(null);
  const rigGroupRef = useRef<THREE.Group | null>(null);
  const jointMeshesRef = useRef<Array<{ bone: THREE.Bone; mesh: THREE.Mesh }>>([]);

  const [playing, setPlaying] = useState(true);
  const [looping, setLooping] = useState(true);
  const [speed, setSpeed] = useState(1);
  const [currentFrame, setCurrentFrame] = useState(0);

  const playingRef = useRef(playing);
  const loopingRef = useRef(looping);
  const speedRef = useRef(speed);
  const frameTimeRef = useRef(0);

  useEffect(() => {
    playingRef.current = playing;
  }, [playing]);
  useEffect(() => {
    loopingRef.current = looping;
  }, [looping]);
  useEffect(() => {
    speedRef.current = speed;
  }, [speed]);

  const clip = preview?.clip ?? null;
  const isFaceOnly = clip?.clip_type === "face_only";
  const fps = clip?.fps ?? 24;
  const frameCount = Math.max(1, clip?.frame_count ?? 1);
  const totalRawFrames = Math.max(1, clip?.total_raw_frames ?? frameCount);
  const durationSec = frameCount / fps;

  const fpsRef = useRef(fps);
  const durationSecRef = useRef(durationSec);
  useEffect(() => {
    fpsRef.current = fps;
  }, [fps]);
  useEffect(() => {
    durationSecRef.current = durationSec;
  }, [durationSec]);

  // Initialize Three.js Scene ONCE (preserving camera angle across re-polish & trim!)
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0f121a);
    scene.fog = new THREE.Fog(0x0f121a, 8, 22);
    sceneRef.current = scene;

    const camera = new THREE.PerspectiveCamera(
      45,
      container.clientWidth / Math.max(1, container.clientHeight),
      0.05,
      100
    );
    camera.position.set(0.0, 1.55, 3.3);
    cameraRef.current = camera;

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(container.clientWidth, container.clientHeight);
    container.innerHTML = "";
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.set(0, 0.95, 0);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.update();
    controlsRef.current = controls;

    // Blender-style dark studio grid + axes + lighting
    const grid = new THREE.GridHelper(12, 24, 0x3b4566, 0x1e2436);
    scene.add(grid);

    const axes = new THREE.AxesHelper(0.65);
    axes.position.set(0, 0.005, 0);
    scene.add(axes);

    const hemi = new THREE.HemisphereLight(0xdbeafe, 0x1e293b, 1.1);
    scene.add(hemi);
    const dir = new THREE.DirectionalLight(0xffffff, 1.2);
    dir.position.set(3, 6, 4);
    scene.add(dir);

    const rigGroup = new THREE.Group();
    scene.add(rigGroup);
    rigGroupRef.current = rigGroup;

    let rafId = 0;
    let lastTs = performance.now();

    const animate = (now: number) => {
      rafId = requestAnimationFrame(animate);
      const dt = Math.min(0.1, (now - lastTs) / 1000);
      lastTs = now;

      if (playingRef.current) {
        const dur =
          actionRef.current?.getClip().duration ??
          Math.max(0.05, durationSecRef.current);
        frameTimeRef.current += dt * speedRef.current;
        if (frameTimeRef.current >= dur) {
          if (loopingRef.current) {
            frameTimeRef.current = frameTimeRef.current % dur;
          } else {
            frameTimeRef.current = dur;
            playingRef.current = false;
            setPlaying(false);
          }
        }
      }

      if (mixerRef.current && actionRef.current) {
        mixerRef.current.setTime(frameTimeRef.current);
      }

      // Sync visual joint spheres to animated bone world positions
      const wp = new THREE.Vector3();
      for (const item of jointMeshesRef.current) {
        item.bone.getWorldPosition(wp);
        item.mesh.position.copy(wp);
      }

      const fIdx = Math.floor(frameTimeRef.current * fpsRef.current);
      setCurrentFrame((prev) => (prev === fIdx ? prev : fIdx));

      controls.update();
      renderer.render(scene, camera);
    };
    rafId = requestAnimationFrame(animate);

    const observer = new ResizeObserver(() => {
      if (!container) return;
      const w = container.clientWidth;
      const h = Math.max(1, container.clientHeight);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    });
    observer.observe(container);

    return () => {
      cancelAnimationFrame(rafId);
      observer.disconnect();
      controls.dispose();
      renderer.dispose();
    };
  }, []);

  // Load BVH into Three.js WITHOUT resetting OrbitControls camera angle!
  useEffect(() => {
    const rigGroup = rigGroupRef.current;
    if (!rigGroup) return;

    // Dispose previous rig objects
    while (rigGroup.children.length > 0) {
      const child = rigGroup.children[0];
      rigGroup.remove(child);
    }
    jointMeshesRef.current = [];
    mixerRef.current = null;
    actionRef.current = null;

    if (!preview?.bvh_content || isFaceOnly) {
      return;
    }

    try {
      const loader = new BVHLoader();
      const parsed = loader.parse(preview.bvh_content);
      const rootBone = parsed.skeleton.bones[0];

      // Convert cm -> meters (0.01) for clean Three.js grid scale
      const skeletonWrapper = new THREE.Group();
      skeletonWrapper.scale.setScalar(0.01);
      skeletonWrapper.add(rootBone);
      rigGroup.add(skeletonWrapper);

      const helper = new THREE.SkeletonHelper(rootBone);
      const helperMat = helper.material as THREE.LineBasicMaterial;
      helperMat.color.setHex(0x38bdf8);
      rigGroup.add(helper);

      // Add glowing joint dots differentiating body/foot vs 30 finger bones
      const bodyGeo = new THREE.SphereGeometry(0.022, 10, 10);
      const footGeo = new THREE.SphereGeometry(0.026, 10, 10);
      const fingerGeo = new THREE.SphereGeometry(0.009, 8, 8);

      const bodyMat = new THREE.MeshStandardMaterial({
        color: 0x818cf8,
        emissive: 0x312e81,
        roughness: 0.35,
      });
      const footMat = new THREE.MeshStandardMaterial({
        color: 0x34d399,
        emissive: 0x065f46,
        roughness: 0.3,
      });
      const fingerMat = new THREE.MeshStandardMaterial({
        color: 0xf472b6,
        emissive: 0x831843,
        roughness: 0.35,
      });

      for (const bone of parsed.skeleton.bones) {
        const lower = bone.name.toLowerCase();
        const isFinger =
          lower.includes("thumb") ||
          lower.includes("index") ||
          lower.includes("middle") ||
          lower.includes("ring") ||
          lower.includes("pinky");
        const isFoot = lower.includes("foot") || lower.includes("toe");

        const mesh = new THREE.Mesh(
          isFinger ? fingerGeo : isFoot ? footGeo : bodyGeo,
          isFinger ? fingerMat : isFoot ? footMat : bodyMat
        );
        rigGroup.add(mesh);
        jointMeshesRef.current.push({ bone, mesh });
      }

      const mixer = new THREE.AnimationMixer(rootBone);
      const action = mixer.clipAction(parsed.clip);
      action.setEffectiveWeight(1.0);
      action.play();

      mixerRef.current = mixer;
      actionRef.current = action;

      // Clamp current playback time cleanly into the updated clip duration
      const clipDur = Math.max(0.01, parsed.clip.duration);
      frameTimeRef.current = Math.min(frameTimeRef.current, clipDur * 0.99);
      mixer.setTime(frameTimeRef.current);
    } catch (err) {
      console.error("Failed to parse BVH in Viewport3D:", err);
    }
  }, [preview?.bvh_content, isFaceOnly]);

  const seekToFrame = (f: number) => {
    const clamped = Math.max(0, Math.min(frameCount - 1, f));
    frameTimeRef.current = clamped / fps;
    setCurrentFrame(clamped);
    if (mixerRef.current) {
      mixerRef.current.setTime(frameTimeRef.current);
    }
  };

  const resetCamera = () => {
    if (cameraRef.current && controlsRef.current) {
      cameraRef.current.position.set(0.0, 1.55, 3.3);
      controlsRef.current.target.set(0, 0.95, 0);
      controlsRef.current.update();
    }
  };

  const currentMuscle =
    preview?.muscle_data?.frames[
      Math.min(
        Math.max(0, currentFrame),
        Math.max(0, (preview?.muscle_data?.frames.length ?? 1) - 1)
      )
    ] ?? null;

  const effectiveTrimEnd =
    polish.trim_end > 0
      ? Math.min(polish.trim_end, totalRawFrames - 1)
      : totalRawFrames - 1;

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-[#0f121a] relative">
      {/* Main Canvas or Expanded Face Monitor */}
      <div className="flex-1 relative min-h-0 overflow-hidden">
        <div
          ref={containerRef}
          className={`w-full h-full ${isFaceOnly ? "hidden" : "block"}`}
        />

        {isFaceOnly && (
          <FaceMonitorWidget
            faceData={preview?.face_data ?? null}
            currentFrame={currentFrame}
            expanded
          />
        )}

        {!clip && (
          <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none text-center px-6">
            <Sparkles className="w-9 h-9 text-indigo-400/50 mb-2" />
            <div className="text-sm font-medium text-slate-300">
              Viewport 3D Siap (51 Tulang + 52 Ekspresi Wajah)
            </div>
            <div className="text-xs text-slate-500 mt-1 max-w-md">
              Pilih klip dari Motion Vault di bawah, atau mulai panen video di
              panel kiri untuk melihat kerangka tubuh, kaki, jari tangan, dan
              ekspresi wajah secara sinkron.
            </div>
          </div>
        )}

        {/* Top-left Clip Info Overlay */}
        {clip && (
          <div className="absolute top-3 left-3 flex items-center gap-2 pointer-events-none">
            <div className="px-2.5 py-1 rounded-lg bg-[#111520]/85 backdrop-blur border border-slate-700/80 text-xs text-slate-100 font-medium shadow">
              {clip.title}
            </div>
            <div className="px-2 py-1 rounded-lg bg-[#111520]/80 border border-slate-800 text-[10px] font-mono text-cyan-300">
              {isFaceOnly
                ? "Face 52 ARKit"
                : polish.include_hands
                ? "51 Bones (Body+Feet+Hands)"
                : "21 Bones (Body+Feet)"}
            </div>
          </div>
        )}

        {/* Reset Camera Button */}
        {!isFaceOnly && (
          <button
            onClick={resetCamera}
            title="Reset Sudut Kamera 3D"
            className="absolute bottom-3 right-3 p-1.5 rounded-lg bg-[#141926]/85 hover:bg-slate-800 border border-slate-700/80 text-slate-300 transition cursor-pointer"
          >
            <RotateCcw className="w-3.5 h-3.5" />
          </button>
        )}

        {/* Face Monitor Overlay (top-right when viewing full_body / blended) */}
        {clip && !isFaceOnly && preview?.face_data && (
          <div className="absolute top-3 right-3">
            <FaceMonitorWidget
              faceData={preview.face_data}
              currentFrame={currentFrame}
            />
          </div>
        )}

        {/* Muscle Drivers Mini Telemetry (bottom-left) */}
        {clip && !isFaceOnly && currentMuscle && polish.include_muscle && (
          <div className="absolute bottom-3 left-3 rounded-lg bg-[#111520]/85 backdrop-blur border border-slate-800 px-2.5 py-1.5 flex items-center gap-3 text-[10px] font-mono text-slate-300">
            <span className="text-indigo-400 font-semibold">MUSCLE</span>
            <span>
              Elbow L/R:{" "}
              {Math.round(currentMuscle.left_elbow_flex * 100)}%/
              {Math.round(currentMuscle.right_elbow_flex * 100)}%
            </span>
            <span>
              Knee L/R:{" "}
              {Math.round(currentMuscle.left_knee_flex * 100)}%/
              {Math.round(currentMuscle.right_knee_flex * 100)}%
            </span>
            <span
              className={
                currentMuscle.impact_spike > 0.35
                  ? "text-amber-400 font-semibold"
                  : "text-slate-400"
              }
            >
              Impact: {Math.round(currentMuscle.impact_spike * 100)}%
            </span>
          </div>
        )}
      </div>

      {/* Transport & Dual-Handle Trim Bar */}
      <div className="shrink-0 bg-[#121620] border-t border-slate-800/90 px-3.5 py-2 space-y-2">
        {/* Playback Controls + Timeline Scrubber */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => seekToFrame(currentFrame - 1)}
            disabled={!clip}
            className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-40 cursor-pointer"
            title="Frame Sebelumnya"
          >
            <SkipBack className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => setPlaying((p) => !p)}
            disabled={!clip}
            className="px-2.5 py-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white disabled:opacity-40 cursor-pointer"
            title="Play / Pause"
          >
            {playing ? (
              <Pause className="w-3.5 h-3.5 fill-current" />
            ) : (
              <Play className="w-3.5 h-3.5 fill-current" />
            )}
          </button>
          <button
            onClick={() => seekToFrame(currentFrame + 1)}
            disabled={!clip}
            className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-40 cursor-pointer"
            title="Frame Berikutnya"
          >
            <SkipForward className="w-3.5 h-3.5" />
          </button>

          <span className="text-[11px] font-mono text-slate-300 min-w-[108px]">
            {fmtTime(currentFrame / fps)} / {fmtTime(durationSec)}
          </span>

          {/* Scrubber */}
          <input
            type="range"
            min={0}
            max={Math.max(0, frameCount - 1)}
            value={Math.min(currentFrame, Math.max(0, frameCount - 1))}
            onChange={(e) => seekToFrame(Number(e.target.value))}
            disabled={!clip}
            className="flex-1 h-1.5 bg-slate-800 rounded cursor-pointer"
          />

          {/* Speed Pills */}
          <div className="flex items-center gap-0.5 bg-slate-900 rounded p-0.5 border border-slate-800">
            {SPEEDS.map((sp) => (
              <button
                key={sp}
                onClick={() => setSpeed(sp)}
                className={`px-1.5 py-0.5 rounded text-[10px] font-mono transition cursor-pointer ${
                  speed === sp
                    ? "bg-indigo-600 text-white"
                    : "text-slate-400 hover:text-slate-200"
                }`}
              >
                {sp}x
              </button>
            ))}
          </div>

          <button
            onClick={() => setLooping((l) => !l)}
            className={`p-1 rounded border transition cursor-pointer ${
              looping
                ? "bg-indigo-500/20 border-indigo-500/60 text-indigo-300"
                : "bg-slate-800 border-slate-700 text-slate-400"
            }`}
            title="Loop Playback"
          >
            <Repeat className="w-3.5 h-3.5" />
          </button>
        </div>

        {/* Non-Destructive Trim Handles (Start / End Frame) */}
        {clip && totalRawFrames > 2 && (
          <div className="flex items-center gap-3 text-[10px] text-slate-400">
            <span className="flex items-center gap-1 text-cyan-400 font-semibold shrink-0">
              <Scissors className="w-3 h-3" />
              TRIM
            </span>
            <div className="flex items-center gap-1.5 flex-1">
              <span className="font-mono shrink-0">In: F{polish.trim_start}</span>
              <input
                type="range"
                min={0}
                max={Math.max(0, effectiveTrimEnd - 1)}
                value={Math.min(polish.trim_start, Math.max(0, effectiveTrimEnd - 1))}
                onChange={(e) =>
                  onTrimChange(Number(e.target.value), effectiveTrimEnd)
                }
                className="flex-1 h-1 bg-slate-800 rounded cursor-pointer"
              />
            </div>
            <div className="flex items-center gap-1.5 flex-1">
              <span className="font-mono shrink-0">Out: F{effectiveTrimEnd}</span>
              <input
                type="range"
                min={Math.min(polish.trim_start + 1, totalRawFrames - 1)}
                max={totalRawFrames - 1}
                value={effectiveTrimEnd}
                onChange={(e) =>
                  onTrimChange(polish.trim_start, Number(e.target.value))
                }
                className="flex-1 h-1 bg-slate-800 rounded cursor-pointer"
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
