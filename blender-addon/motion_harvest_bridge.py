bl_info = {
    "name": "MotionHarvest Studio Bridge",
    "author": "MotionHarvest Studio",
    "version": (1, 0, 0),
    "blender": (4, 0, 0),
    "location": "View3D > Sidebar (N) > MotionHarvest",
    "description": "1-Click bridge receiving 51-bone BVH, 52 ARKit Facial Shape Keys, Muscle Drivers, Auto-Retarget (Mixamo/Rigify/VRoid), and Stepped Anime F-Curves from MotionHarvest Studio.",
    "category": "Animation",
}

import json
import os
import queue
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import bpy
from bpy.props import EnumProperty, IntProperty, PointerProperty

# ---------------------------------------------------------------------------
# Auto-Retarget Bone Tables (MotionHarvest 51-Bone Standard -> Target Rigs)
# ---------------------------------------------------------------------------

RETARGET_MAPS = {
    "mixamo": {
        "Hips": "mixamorig:Hips",
        "Spine": "mixamorig:Spine",
        "Chest": "mixamorig:Spine2",
        "Neck": "mixamorig:Neck",
        "Head": "mixamorig:Head",
        "LeftShoulder": "mixamorig:LeftShoulder",
        "LeftUpperArm": "mixamorig:LeftArm",
        "LeftLowerArm": "mixamorig:LeftForeArm",
        "LeftHand": "mixamorig:LeftHand",
        "RightShoulder": "mixamorig:RightShoulder",
        "RightUpperArm": "mixamorig:RightArm",
        "RightLowerArm": "mixamorig:RightForeArm",
        "RightHand": "mixamorig:RightHand",
        "LeftUpperLeg": "mixamorig:LeftUpLeg",
        "LeftLowerLeg": "mixamorig:LeftLeg",
        "LeftFoot": "mixamorig:LeftFoot",
        "LeftToe": "mixamorig:LeftToeBase",
        "RightUpperLeg": "mixamorig:RightUpLeg",
        "RightLowerLeg": "mixamorig:RightLeg",
        "RightFoot": "mixamorig:RightFoot",
        "RightToe": "mixamorig:RightToeBase",
        # Fingers
        "LeftThumb1": "mixamorig:LeftHandThumb1",
        "LeftThumb2": "mixamorig:LeftHandThumb2",
        "LeftThumb3": "mixamorig:LeftHandThumb3",
        "LeftIndex1": "mixamorig:LeftHandIndex1",
        "LeftIndex2": "mixamorig:LeftHandIndex2",
        "LeftIndex3": "mixamorig:LeftHandIndex3",
        "LeftMiddle1": "mixamorig:LeftHandMiddle1",
        "LeftMiddle2": "mixamorig:LeftHandMiddle2",
        "LeftMiddle3": "mixamorig:LeftHandMiddle3",
        "LeftRing1": "mixamorig:LeftHandRing1",
        "LeftRing2": "mixamorig:LeftHandRing2",
        "LeftRing3": "mixamorig:LeftHandRing3",
        "LeftPinky1": "mixamorig:LeftHandPinky1",
        "LeftPinky2": "mixamorig:LeftHandPinky2",
        "LeftPinky3": "mixamorig:LeftHandPinky3",
        "RightThumb1": "mixamorig:RightHandThumb1",
        "RightThumb2": "mixamorig:RightHandThumb2",
        "RightThumb3": "mixamorig:RightHandThumb3",
        "RightIndex1": "mixamorig:RightHandIndex1",
        "RightIndex2": "mixamorig:RightHandIndex2",
        "RightIndex3": "mixamorig:RightHandIndex3",
        "RightMiddle1": "mixamorig:RightHandMiddle1",
        "RightMiddle2": "mixamorig:RightHandMiddle2",
        "RightMiddle3": "mixamorig:RightHandMiddle3",
        "RightRing1": "mixamorig:RightHandRing1",
        "RightRing2": "mixamorig:RightHandRing2",
        "RightRing3": "mixamorig:RightHandRing3",
        "RightPinky1": "mixamorig:RightHandPinky1",
        "RightPinky2": "mixamorig:RightHandPinky2",
        "RightPinky3": "mixamorig:RightHandPinky3",
    },
    "rigify": {
        "Hips": "torso",
        "Spine": "spine_fk.001",
        "Chest": "chest",
        "Neck": "neck",
        "Head": "head",
        "LeftShoulder": "shoulder.L",
        "LeftUpperArm": "upper_arm_fk.L",
        "LeftLowerArm": "forearm_fk.L",
        "LeftHand": "hand_fk.L",
        "RightShoulder": "shoulder.R",
        "RightUpperArm": "upper_arm_fk.R",
        "RightLowerArm": "forearm_fk.R",
        "RightHand": "hand_fk.R",
        "LeftUpperLeg": "thigh_fk.L",
        "LeftLowerLeg": "shin_fk.L",
        "LeftFoot": "foot_fk.L",
        "LeftToe": "toe_fk.L",
        "RightUpperLeg": "thigh_fk.R",
        "RightLowerLeg": "shin_fk.R",
        "RightFoot": "foot_fk.R",
        "RightToe": "toe_fk.R",
        "LeftThumb1": "thumb.01.L",
        "LeftThumb2": "thumb.02.L",
        "LeftThumb3": "thumb.03.L",
        "LeftIndex1": "f_index.01.L",
        "LeftIndex2": "f_index.02.L",
        "LeftIndex3": "f_index.03.L",
        "LeftMiddle1": "f_middle.01.L",
        "LeftMiddle2": "f_middle.02.L",
        "LeftMiddle3": "f_middle.03.L",
        "LeftRing1": "f_ring.01.L",
        "LeftRing2": "f_ring.02.L",
        "LeftRing3": "f_ring.03.L",
        "LeftPinky1": "f_pinky.01.L",
        "LeftPinky2": "f_pinky.02.L",
        "LeftPinky3": "f_pinky.03.L",
        "RightThumb1": "thumb.01.R",
        "RightThumb2": "thumb.02.R",
        "RightThumb3": "thumb.03.R",
        "RightIndex1": "f_index.01.R",
        "RightIndex2": "f_index.02.R",
        "RightIndex3": "f_index.03.R",
        "RightMiddle1": "f_middle.01.R",
        "RightMiddle2": "f_middle.02.R",
        "RightMiddle3": "f_middle.03.R",
        "RightRing1": "f_ring.01.R",
        "RightRing2": "f_ring.02.R",
        "RightRing3": "f_ring.03.R",
        "RightPinky1": "f_pinky.01.R",
        "RightPinky2": "f_pinky.02.R",
        "RightPinky3": "f_pinky.03.R",
    },
    "vroid": {
        "Hips": "J_Bip_C_Hips",
        "Spine": "J_Bip_C_Spine",
        "Chest": "J_Bip_C_UpperChest",
        "Neck": "J_Bip_C_Neck",
        "Head": "J_Bip_C_Head",
        "LeftShoulder": "J_Bip_L_Shoulder",
        "LeftUpperArm": "J_Bip_L_UpperArm",
        "LeftLowerArm": "J_Bip_L_LowerArm",
        "LeftHand": "J_Bip_L_Hand",
        "RightShoulder": "J_Bip_R_Shoulder",
        "RightUpperArm": "J_Bip_R_UpperArm",
        "RightLowerArm": "J_Bip_R_LowerArm",
        "RightHand": "J_Bip_R_Hand",
        "LeftUpperLeg": "J_Bip_L_UpperLeg",
        "LeftLowerLeg": "J_Bip_L_LowerLeg",
        "LeftFoot": "J_Bip_L_Foot",
        "LeftToe": "J_Bip_L_ToeBase",
        "RightUpperLeg": "J_Bip_R_UpperLeg",
        "RightLowerLeg": "J_Bip_R_LowerLeg",
        "RightFoot": "J_Bip_R_Foot",
        "RightToe": "J_Bip_R_ToeBase",
        "LeftThumb1": "J_Bip_L_Thumb1",
        "LeftThumb2": "J_Bip_L_Thumb2",
        "LeftThumb3": "J_Bip_L_Thumb3",
        "LeftIndex1": "J_Bip_L_Index1",
        "LeftIndex2": "J_Bip_L_Index2",
        "LeftIndex3": "J_Bip_L_Index3",
        "LeftMiddle1": "J_Bip_L_Middle1",
        "LeftMiddle2": "J_Bip_L_Middle2",
        "LeftMiddle3": "J_Bip_L_Middle3",
        "LeftRing1": "J_Bip_L_Ring1",
        "LeftRing2": "J_Bip_L_Ring2",
        "LeftRing3": "J_Bip_L_Ring3",
        "LeftPinky1": "J_Bip_L_Little1",
        "LeftPinky2": "J_Bip_L_Little2",
        "LeftPinky3": "J_Bip_L_Little3",
        "RightThumb1": "J_Bip_R_Thumb1",
        "RightThumb2": "J_Bip_R_Thumb2",
        "RightThumb3": "J_Bip_R_Thumb3",
        "RightIndex1": "J_Bip_R_Index1",
        "RightIndex2": "J_Bip_R_Index2",
        "RightIndex3": "J_Bip_R_Index3",
        "RightMiddle1": "J_Bip_R_Middle1",
        "RightMiddle2": "J_Bip_R_Middle2",
        "RightMiddle3": "J_Bip_R_Middle3",
        "RightRing1": "J_Bip_R_Ring1",
        "RightRing2": "J_Bip_R_Ring2",
        "RightRing3": "J_Bip_R_Ring3",
        "RightPinky1": "J_Bip_R_Little1",
        "RightPinky2": "J_Bip_R_Little2",
        "RightPinky3": "J_Bip_R_Little3",
    },
}

# VRoid / VRM Shape Key aliases for ARKit 52 blendshapes
VROID_SHAPEKEY_ALIASES = {
    "mouthSmileLeft": ["Fcl_MTH_Fun", "Fcl_ALL_Joy", "Fcl_MTH_Joy"],
    "mouthSmileRight": ["Fcl_MTH_Fun", "Fcl_ALL_Joy", "Fcl_MTH_Joy"],
    "mouthFrownLeft": ["Fcl_MTH_Sorrow", "Fcl_ALL_Sorrow"],
    "mouthFrownRight": ["Fcl_MTH_Sorrow", "Fcl_ALL_Sorrow"],
    "browDownLeft": ["Fcl_BRW_Angry", "Fcl_ALL_Angry"],
    "browDownRight": ["Fcl_BRW_Angry", "Fcl_ALL_Angry"],
    "browInnerUp": ["Fcl_BRW_Sorrow", "Fcl_BRW_Surprised"],
    "eyeBlinkLeft": ["Fcl_EYE_Close_L", "Fcl_EYE_Close"],
    "eyeBlinkRight": ["Fcl_EYE_Close_R", "Fcl_EYE_Close"],
    "eyeWideLeft": ["Fcl_EYE_Surprised"],
    "eyeWideRight": ["Fcl_EYE_Surprised"],
    "jawOpen": ["Fcl_MTH_A"],
    "mouthFunnel": ["Fcl_MTH_O"],
    "mouthPucker": ["Fcl_MTH_U"],
}

# ---------------------------------------------------------------------------
# Thread-safe Bridge Server State
# ---------------------------------------------------------------------------

_server_instance = None
_server_thread = None
_job_queue = queue.Queue()
_timer_registered = False


def _get_selected_armature_name():
    scene = bpy.context.scene
    if scene and hasattr(scene, "mh_bridge") and scene.mh_bridge.target_armature:
        return scene.mh_bridge.target_armature.name
    obj = bpy.context.active_object
    if obj and obj.type == "ARMATURE":
        return obj.name
    return None


def _set_action_constant_interpolation(action):
    if not action:
        return
    for fcurve in action.fcurves:
        for kp in fcurve.keyframe_points:
            kp.interpolation = "CONSTANT"


def _apply_retarget_and_bake(src_arm, target_arm, preset_name, frame_start, frame_end, clip_name):
    bone_map = RETARGET_MAPS.get(preset_name, {})
    if not bone_map:
        return

    constraints_added = []
    for src_name, dst_name in bone_map.items():
        if src_name not in src_arm.pose.bones or dst_name not in target_arm.pose.bones:
            continue
        pbone = target_arm.pose.bones[dst_name]
        c_rot = pbone.constraints.new("COPY_ROTATION")
        c_rot.target = src_arm
        c_rot.subtarget = src_name
        c_rot.target_space = "LOCAL"
        c_rot.owner_space = "LOCAL"
        constraints_added.append((pbone, c_rot))

        if src_name == "Hips":
            c_loc = pbone.constraints.new("COPY_LOCATION")
            c_loc.target = src_arm
            c_loc.subtarget = src_name
            c_loc.target_space = "WORLD"
            c_loc.owner_space = "WORLD"
            constraints_added.append((pbone, c_loc))

    if not constraints_added:
        return

    bpy.ops.object.select_all(action="DESELECT")
    target_arm.select_set(True)
    bpy.context.view_layer.objects.active = target_arm
    bpy.ops.nla.bake(
        frame_start=int(frame_start),
        frame_end=int(frame_end),
        only_selected=False,
        visual_keying=True,
        clear_constraints=True,
        use_current_action=False,
        bake_types={"POSE"},
    )
    if target_arm.animation_data and target_arm.animation_data.action:
        target_arm.animation_data.action.name = f"MH_{clip_name}_{preset_name}"


def _apply_face_shapekeys(face_mesh, face_path, stepped_anime):
    if not face_mesh or face_mesh.type != "MESH":
        return 0
    if not face_mesh.data.shape_keys:
        return 0
    if not face_path or not os.path.isfile(face_path):
        return 0

    with open(face_path, "r", encoding="utf-8") as fh:
        face_data = json.load(fh)

    frames = face_data.get("frames", [])
    if not frames:
        return 0

    key_blocks = face_mesh.data.shape_keys.key_blocks
    existing_keys = {kb.name: kb for kb in key_blocks}
    existing_lower = {kb.name.lower(): kb for kb in key_blocks}
    keyframed_count = 0

    for frame_idx, blend_map in enumerate(frames):
        frame_num = frame_idx + 1
        # Accumulate values per target shape key (handles multiple ARKit -> single VRoid key)
        resolved = {}
        for arkit_key, val in blend_map.items():
            v = max(0.0, min(1.0, float(val)))
            if arkit_key in existing_keys:
                resolved[arkit_key] = max(resolved.get(arkit_key, 0.0), v)
            elif arkit_key.lower() in existing_lower:
                real_name = existing_lower[arkit_key.lower()].name
                resolved[real_name] = max(resolved.get(real_name, 0.0), v)
            else:
                for alias in VROID_SHAPEKEY_ALIASES.get(arkit_key, []):
                    if alias in existing_keys:
                        resolved[alias] = max(resolved.get(alias, 0.0), v)

        for kb_name, val in resolved.items():
            kb = existing_keys[kb_name]
            kb.value = val
            kb.keyframe_insert(data_path="value", frame=frame_num)
            keyframed_count += 1

    if stepped_anime and face_mesh.data.shape_keys.animation_data:
        _set_action_constant_interpolation(face_mesh.data.shape_keys.animation_data.action)

    return keyframed_count


def _apply_muscle_drivers(target_obj, muscle_path, stepped_anime):
    if not target_obj or not muscle_path or not os.path.isfile(muscle_path):
        return 0
    with open(muscle_path, "r", encoding="utf-8") as fh:
        muscle_data = json.load(fh)

    frames = muscle_data.get("frames", [])
    if not frames:
        return 0

    prop_keys = [
        ("mh_left_elbow_flex", "left_elbow_flex"),
        ("mh_right_elbow_flex", "right_elbow_flex"),
        ("mh_left_knee_flex", "left_knee_flex"),
        ("mh_right_knee_flex", "right_knee_flex"),
        ("mh_shoulder_raise_l", "shoulder_raise_l"),
        ("mh_shoulder_raise_r", "shoulder_raise_r"),
        ("mh_angular_velocity", "angular_velocity"),
        ("mh_impact_spike", "impact_spike"),
    ]

    for prop_name, _ in prop_keys:
        target_obj[prop_name] = 0.0
        try:
            ui = target_obj.id_properties_ui(prop_name)
            ui.update(min=0.0, max=1.0, soft_min=0.0, soft_max=1.0)
        except Exception:
            pass

    for frame_idx, mframe in enumerate(frames):
        frame_num = frame_idx + 1
        for prop_name, json_key in prop_keys:
            target_obj[prop_name] = float(mframe.get(json_key, 0.0))
            target_obj.keyframe_insert(data_path=f'["{prop_name}"]', frame=frame_num)

    if stepped_anime and target_obj.animation_data:
        _set_action_constant_interpolation(target_obj.animation_data.action)

    return len(frames)


def _execute_import_on_main_thread(payload):
    clip_name = payload.get("clip_name", "MotionHarvest_Clip")
    bvh_path = payload.get("bvh_path")
    face_path = payload.get("face_path")
    muscle_path = payload.get("muscle_path")
    retarget_preset = payload.get("retarget_preset", "raw")
    global_scale = float(payload.get("global_scale", 1.0))
    stepped_anime = bool(payload.get("stepped_anime", False))
    apply_muscle = bool(payload.get("apply_muscle_drivers", True))

    scene = bpy.context.scene
    props = getattr(scene, "mh_bridge", None)
    target_arm = props.target_armature if props else None
    target_face = props.target_face_mesh if props else None

    if target_arm is None and bpy.context.active_object and bpy.context.active_object.type == "ARMATURE":
        target_arm = bpy.context.active_object

    imported_arm = None
    frame_start = 1
    frame_end = scene.frame_end

    # 1. Import 21 or 51-bone BVH
    if bvh_path and os.path.isfile(bvh_path):
        before_objs = set(bpy.data.objects)
        bpy.ops.import_anim.bvh(
            filepath=bvh_path,
            global_scale=global_scale * 0.01,
            axis_forward="-Z",
            axis_up="Y",
            use_fps_scale=False,
            update_scene_fps=True,
            update_scene_duration=True,
        )
        after_objs = set(bpy.data.objects) - before_objs
        for obj in after_objs:
            if obj.type == "ARMATURE":
                imported_arm = obj
                obj.name = f"MH_{clip_name}"
                if obj.animation_data and obj.animation_data.action:
                    act = obj.animation_data.action
                    act.name = f"MH_{clip_name}_Raw"
                    r0, r1 = act.frame_range
                    frame_start = int(r0)
                    frame_end = int(r1)
                    if stepped_anime:
                        _set_action_constant_interpolation(act)
                break

    # 2. Optional Auto-Retarget onto selected character rig (Mixamo / Rigify / VRoid)
    driver_target = imported_arm
    if imported_arm and target_arm and target_arm != imported_arm and retarget_preset in RETARGET_MAPS:
        _apply_retarget_and_bake(
            imported_arm, target_arm, retarget_preset, frame_start, frame_end, clip_name
        )
        if stepped_anime and target_arm.animation_data:
            _set_action_constant_interpolation(target_arm.animation_data.action)
        driver_target = target_arm

    # 3. Apply 52 ARKit Facial Shape Keys (with VRoid/VRM alias support)
    if target_face is None:
        # Auto-discover first mesh in scene that has shape keys
        for obj in bpy.data.objects:
            if obj.type == "MESH" and obj.data.shape_keys and len(obj.data.shape_keys.key_blocks) > 1:
                target_face = obj
                break
    face_keys = _apply_face_shapekeys(target_face, face_path, stepped_anime)

    # 4. Apply Muscle Driver Curves as keyframed Custom Properties
    muscle_frames = 0
    if apply_muscle and driver_target:
        muscle_frames = _apply_muscle_drivers(driver_target, muscle_path, stepped_anime)

    parts = [f"Impor '{clip_name}' selesai"]
    if imported_arm:
        parts.append(f"BVH ({retarget_preset})")
    if face_keys > 0:
        parts.append(f"Face ShapeKeys ({face_keys} keys)")
    if muscle_frames > 0:
        parts.append(f"Muscle Drivers ({muscle_frames}f)")
    if stepped_anime:
        parts.append("Stepped Anime")
    return " | ".join(parts)


def _poll_job_queue():
    while not _job_queue.empty():
        payload, reply_event, reply_box = _job_queue.get()
        try:
            msg = _execute_import_on_main_thread(payload)
            reply_box["ok"] = True
            reply_box["message"] = msg
        except Exception as exc:
            reply_box["ok"] = False
            reply_box["error"] = str(exc)
        finally:
            reply_event.set()
    return 0.15 if _server_instance is not None else None


class _BridgeHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        return  # Keep Blender console clean

    def _send_json(self, code, data):
        body = json.dumps(data).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/ping":
            self._send_json(
                200,
                {
                    "ok": True,
                    "blender_version": ".".join(str(v) for v in bpy.app.version),
                    "selected_armature": _get_selected_armature_name(),
                },
            )
            return
        self._send_json(404, {"ok": False, "error": "Not found"})

    def do_POST(self):
        if self.path != "/import_performance":
            self._send_json(404, {"ok": False, "error": "Not found"})
            return

        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length > 0 else b"{}"
        try:
            payload = json.loads(raw.decode("utf-8"))
        except Exception as exc:
            self._send_json(400, {"ok": False, "error": f"Invalid JSON: {exc}"})
            return

        reply_event = threading.Event()
        reply_box = {}
        _job_queue.put((payload, reply_event, reply_box))
        finished = reply_event.wait(timeout=28.0)
        if not finished:
            self._send_json(504, {"ok": False, "error": "Blender main thread timeout"})
            return

        status = 200 if reply_box.get("ok") else 500
        self._send_json(status, reply_box)


def _start_server(port):
    global _server_instance, _server_thread, _timer_registered
    if _server_instance is not None:
        return
    _server_instance = ThreadingHTTPServer(("127.0.0.1", int(port)), _BridgeHandler)
    _server_thread = threading.Thread(target=_server_instance.serve_forever, daemon=True)
    _server_thread.start()
    if not _timer_registered:
        bpy.app.timers.register(_poll_job_queue, persistent=True)
        _timer_registered = True


def _stop_server():
    global _server_instance, _server_thread, _timer_registered
    if _server_instance is not None:
        _server_instance.shutdown()
        _server_instance.server_close()
        _server_instance = None
        _server_thread = None
        _timer_registered = False


# ---------------------------------------------------------------------------
# Blender UI Properties, Operators & Sidebar Panel (N -> MotionHarvest)
# ---------------------------------------------------------------------------


def _poll_armature(self, obj):
    return obj.type == "ARMATURE"


def _poll_mesh(self, obj):
    return obj.type == "MESH"


class MHBridgeSettings(bpy.types.PropertyGroup):
    port: IntProperty(
        name="Port",
        default=9877,
        min=1024,
        max=65535,
        description="Local HTTP port for MotionHarvest Studio Bridge",
    )
    target_armature: PointerProperty(
        name="Target Armature",
        type=bpy.types.Object,
        poll=_poll_armature,
        description="Character rig to auto-retarget incoming BVH animations onto",
    )
    target_face_mesh: PointerProperty(
        name="Face Mesh",
        type=bpy.types.Object,
        poll=_poll_mesh,
        description="Mesh containing ARKit or VRoid Shape Keys for facial animation",
    )
    default_preset: EnumProperty(
        name="Default Rig",
        items=[
            ("raw", "Raw BVH", "Import raw 21/51-bone BVH armature directly"),
            ("mixamo", "Mixamo", "Auto-retarget onto Mixamo rig"),
            ("rigify", "Rigify", "Auto-retarget onto Blender Rigify rig"),
            ("vroid", "VRoid / VRM", "Auto-retarget onto VRoid / Anime 3D VRM rig"),
        ],
        default="vroid",
    )


class MH_OT_ToggleBridge(bpy.types.Operator):
    bl_idname = "motionharvest.toggle_bridge"
    bl_label = "Start / Stop MotionHarvest Bridge"

    def execute(self, context):
        props = context.scene.mh_bridge
        if _server_instance is None:
            try:
                _start_server(props.port)
                self.report({"INFO"}, f"MotionHarvest Bridge aktif di 127.0.0.1:{props.port}")
            except Exception as exc:
                self.report({"ERROR"}, f"Gagal memulai server: {exc}")
        else:
            _stop_server()
            self.report({"INFO"}, "MotionHarvest Bridge dihentikan")
        return {"FINISHED"}


class MH_PT_BridgePanel(bpy.types.Panel):
    bl_label = "MotionHarvest Studio"
    bl_idname = "MH_PT_bridge_panel"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "MotionHarvest"

    def draw(self, context):
        layout = self.layout
        props = context.scene.mh_bridge
        running = _server_instance is not None

        box = layout.box()
        row = box.row()
        if running:
            row.label(text=f"Status: Terhubung (:{props.port})", icon="CHECKMARK")
        else:
            row.label(text="Status: Nonaktif", icon="X")

        box.prop(props, "port")
        box.operator(
            "motionharvest.toggle_bridge",
            text="Stop Bridge" if running else "Start Bridge",
            icon="PAUSE" if running else "PLAY",
        )

        layout.separator()
        col = layout.column(align=True)
        col.label(text="Target Karakter (Auto-Retarget & Wajah):")
        col.prop(props, "target_armature")
        col.prop(props, "target_face_mesh")
        col.prop(props, "default_preset")


classes = (MHBridgeSettings, MH_OT_ToggleBridge, MH_PT_BridgePanel)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.Scene.mh_bridge = PointerProperty(type=MHBridgeSettings)


def unregister():
    _stop_server()
    if hasattr(bpy.types.Scene, "mh_bridge"):
        del bpy.types.Scene.mh_bridge
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)


if __name__ == "__main__":
    register()
