// The trace player: a scene (`lab.scene.v0`) drawn once, then driven
// entirely by a simulation trace (`lab.sim-trace.v0`). The viewer computes
// nothing — every state change it shows comes from an event, and seeking
// is replaying events from the start up to the chosen time.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

// ---------- document types ----------

type Geometry =
  | { shape: "box"; x: number; y: number; z: number }
  | { shape: "cylinder"; diameter: number; height: number }
  | { shape: "mesh"; gltf?: string; usd?: string; fallback: [number, number, number] };

interface SceneNode {
  id: string;
  semantic: { kind: string; [key: string]: unknown };
  translation: [number, number, number];
  rotation_z_deg?: number;
  geometry?: Geometry;
  children?: SceneNode[];
}

interface SceneDocument {
  format: string;
  name: string;
  root: SceneNode;
}

interface TimedEvent {
  t: number;
  event: string;
  [key: string]: unknown;
}

interface TraceDocument {
  format: string;
  durations: string;
  events: TimedEvent[];
  summary: {
    total_seconds: number;
    attended_seconds: number;
    walkaway_seconds: number;
    nodes: number;
    attention_windows: { node: string; from_seconds: number; to_seconds: number }[];
  };
}

// ---------- three.js scene construction ----------

const MATERIALS: Record<string, THREE.MeshStandardMaterial> = {
  deck: new THREE.MeshStandardMaterial({ color: 0x8c8f94, roughness: 0.9 }),
  carrier: new THREE.MeshStandardMaterial({ color: 0x54575e, roughness: 0.8 }),
  labware: new THREE.MeshStandardMaterial({ color: 0xebecef, roughness: 0.5 }),
  tips: new THREE.MeshStandardMaterial({ color: 0xe69933, roughness: 0.6 }),
  well: new THREE.MeshStandardMaterial({
    color: 0x4d8ce6,
    roughness: 0.3,
    transparent: true,
    opacity: 0.45,
  }),
  station: new THREE.MeshStandardMaterial({ color: 0x9ea3ab, roughness: 0.9 }),
  // Walls render inside-out so the camera sees into the room from anywhere.
  room: new THREE.MeshStandardMaterial({
    color: 0xd1cfc7,
    roughness: 0.95,
    side: THREE.BackSide,
  }),
  frame: new THREE.MeshStandardMaterial({ color: 0x2a2c33, roughness: 0.45, metalness: 0.8 }),
  panel: new THREE.MeshStandardMaterial({ color: 0xcccfd4, roughness: 0.55, metalness: 0.3 }),
  glass: new THREE.MeshPhysicalMaterial({
    color: 0xa6c0cc,
    roughness: 0.05,
    metalness: 0.0,
    transparent: true,
    opacity: 0.22,
  }),
  accent: new THREE.MeshStandardMaterial({ color: 0x1a8cbf, roughness: 0.3 }),
};

function materialFor(node: SceneNode): THREE.MeshStandardMaterial {
  const kind = node.semantic.kind;
  if (kind === "part") {
    return MATERIALS[String(node.semantic.material)] ?? MATERIALS.station;
  }
  if (kind === "room") return MATERIALS.room;
  if (kind === "deck") return MATERIALS.deck;
  if (kind === "carrier") return MATERIALS.carrier;
  if (kind === "labware" || kind === "well") {
    const catalog = String(node.semantic.catalog ?? node.id);
    if (catalog.includes("tip") || node.id.includes("tip")) return MATERIALS.tips;
    return kind === "well" ? MATERIALS.well : MATERIALS.labware;
  }
  return MATERIALS.station;
}

const assetLoader = new GLTFLoader();

/** Swaps a fallback box for its real asset once it loads; the box stays
 * on failure, which is the registry's contract. */
function loadAsset(url: string, group: THREE.Group, fallback: THREE.Mesh): void {
  assetLoader.load(
    url,
    (loaded) => {
      loaded.scene.traverse((child) => {
        if (child instanceof THREE.Mesh) {
          child.castShadow = true;
          child.receiveShadow = true;
        }
      });
      group.remove(fallback);
      group.add(loaded.scene);
    },
    undefined,
    () => {
      /* keep the fallback box */
    },
  );
}

const byId = new Map<string, THREE.Object3D>();
const stationMeshes = new Map<string, THREE.Mesh>();
const stationHeights = new Map<string, number>();

// Motion state, all cosmetic: targets come from trace events; these only
// decide how the picture gets from one true state to the next.
const heads = new Map<string, THREE.Group>();
const headTargets = new Map<string, THREE.Vector3>();
const thermalActive = new Set<string>();
interface Tween {
  object: THREE.Object3D;
  from: THREE.Vector3;
  to: THREE.Vector3;
  started: number;
}
let tweens: Tween[] = [];
const TWEEN_SECONDS = 0.7;

function buildNode(node: SceneNode): THREE.Object3D {
  const group = new THREE.Group();
  group.name = node.id;
  group.position.set(...node.translation);
  if (node.rotation_z_deg) {
    group.rotation.z = (node.rotation_z_deg * Math.PI) / 180;
  }
  byId.set(node.id, group);

  if (node.geometry) {
    let mesh: THREE.Mesh;
    if (node.geometry.shape === "box" || node.geometry.shape === "mesh") {
      const [x, y, z] =
        node.geometry.shape === "box"
          ? [node.geometry.x, node.geometry.y, node.geometry.z]
          : node.geometry.fallback;
      mesh = new THREE.Mesh(new THREE.BoxGeometry(x, y, z), materialFor(node).clone());
      // The scene convention puts a box's minimum corner at the node
      // origin; three centers geometry.
      mesh.position.set(x / 2, y / 2, z / 2);
      if (node.geometry.shape === "mesh" && node.geometry.gltf) {
        loadAsset(node.geometry.gltf, group, mesh);
      }
    } else {
      const { diameter, height } = node.geometry;
      mesh = new THREE.Mesh(
        new THREE.CylinderGeometry(diameter / 2, diameter / 2, height, 20),
        materialFor(node).clone(),
      );
      // Cylinders stand on the node origin along lab Z; three's cylinder
      // axis is local Y.
      mesh.rotation.x = Math.PI / 2;
      mesh.position.set(0, 0, height / 2);
    }
    const isWell = node.semantic.kind === "well";
    const isRoom = node.semantic.kind === "room";
    const isGlass = node.semantic.kind === "part" && node.semantic.material === "glass";
    mesh.castShadow = !isWell && !isRoom && !isGlass;
    mesh.receiveShadow = true;
    group.add(mesh);
    if (node.semantic.kind === "station") {
      stationMeshes.set(node.id, mesh);
      stationHeights.set(
        node.id,
        node.geometry.shape === "box"
          ? node.geometry.z
          : node.geometry.shape === "cylinder"
            ? node.geometry.height
            : node.geometry.fallback[2],
      );
    }
  }
  for (const child of node.children ?? []) {
    group.add(buildNode(child));
  }
  return group;
}

// ---------- playback state ----------

interface LabwareHome {
  parent: THREE.Object3D;
  position: THREE.Vector3;
}

const homes = new Map<string, LabwareHome>();
let trace: TraceDocument | null = null;
let sceneDoc: SceneDocument | null = null;
let simTime = 0;
let playing = false;
let speed = 60;
let applied = 0; // events applied so far, for incremental playback

function rememberHomes(): void {
  homes.clear();
  for (const [id, object] of byId) {
    homes.set(id, { parent: object.parent!, position: object.position.clone() });
  }
}

function resetState(): void {
  for (const [id, home] of homes) {
    const object = byId.get(id)!;
    if (object.parent !== home.parent) home.parent.add(object);
    object.position.copy(home.position);
  }
  for (const mesh of stationMeshes.values()) {
    (mesh.material as THREE.MeshStandardMaterial).emissive.setHex(0x000000);
  }
  for (const head of heads.values()) head.visible = false;
  headTargets.clear();
  thermalActive.clear();
  tweens = [];
  hideAttention();
  applied = 0;
}

/** The pipetting head a liquid handler shows while its frames run: a
 * carriage block with a tip below it, gliding between frame targets. */
function ensureHead(station: string): THREE.Group {
  let head = heads.get(station);
  if (head) return head;
  head = new THREE.Group();
  const carriage = new THREE.Mesh(
    new THREE.BoxGeometry(60, 60, 110),
    new THREE.MeshStandardMaterial({ color: 0x2f3239, roughness: 0.4 }),
  );
  carriage.position.z = 55;
  const tip = new THREE.Mesh(
    new THREE.CylinderGeometry(2.5, 1.2, 70, 10),
    new THREE.MeshStandardMaterial({ color: 0xd8dbe2, roughness: 0.3 }),
  );
  tip.rotation.x = Math.PI / 2;
  tip.position.z = -35;
  head.add(carriage, tip);
  head.visible = false;
  (byId.get(station) ?? labRoot).add(head);
  heads.set(station, head);
  return head;
}

/** Moves labware to a station: same world pose logic a handoff implies.
 * Animated during playback, snapped when seeking. */
function moveLabware(labware: string, to: string, animate: boolean): void {
  const object = byId.get(labware);
  const station = byId.get(to);
  if (!object || !station || !object.parent) return;
  // Work in the lab frame (Z-up millimeters): seat the plate on top of
  // the destination's body so it reads as "on" the instrument rather
  // than inside it, then map into the labware's parent frame.
  const point = new THREE.Vector3();
  station.getWorldPosition(point);
  labRoot.worldToLocal(point);
  point.z += (stationHeights.get(to) ?? 0) + 5;
  labRoot.localToWorld(point);
  object.parent.worldToLocal(point);
  if (animate) {
    tweens = tweens.filter((tween) => tween.object !== object);
    tweens.push({
      object,
      from: object.position.clone(),
      to: point.clone(),
      started: performance.now(),
    });
  } else {
    object.position.copy(point);
  }
}

function applyEvent(timed: TimedEvent, animate: boolean): void {
  switch (timed.event) {
    case "labware-moved":
      moveLabware(String(timed.labware), String(timed.to), animate);
      break;
    case "door-opened":
      thermalActive.delete(String(timed.station));
      tintStation(String(timed.station), 0x664400);
      break;
    case "door-closed":
      tintStation(String(timed.station), 0x000000);
      break;
    case "thermal-running":
      thermalActive.add(String(timed.station));
      tintStation(String(timed.station), 0x551111);
      break;
    case "frame": {
      if (typeof timed.x_mm === "number" && typeof timed.y_mm === "number") {
        const station = String(timed.station);
        const head = ensureHead(station);
        head.visible = true;
        const target = new THREE.Vector3(timed.x_mm, timed.y_mm, 260);
        headTargets.set(station, target);
        if (!animate) head.position.copy(target);
      }
      break;
    }
    case "node-completed":
      for (const head of heads.values()) head.visible = false;
      break;
  }
  // Caption: the latest human-meaningful line.
  const caption = describe(timed);
  if (caption) document.getElementById("caption")!.textContent = caption;
}

function tintStation(station: string, hex: number): void {
  const mesh = stationMeshes.get(station);
  if (mesh) (mesh.material as THREE.MeshStandardMaterial).emissive.setHex(hex);
}

function describe(timed: TimedEvent): string | null {
  switch (timed.event) {
    case "node-started":
      return `▸ ${timed.id}`;
    case "program-started":
      return `${timed.station}: ${timed.title}`;
    case "attention-required":
      return `by hand — ${timed.prompt}`;
    case "labware-moved":
      return `${timed.labware}: ${timed.from} → ${timed.to}`;
    case "thermal-hold":
      return `holding ${timed.celsius} °C`;
    default:
      return null;
  }
}

function showAttention(prompt: string): void {
  const panel = document.getElementById("attention")!;
  panel.classList.add("active");
  document.getElementById("attention-text")!.textContent = prompt;
}

function hideAttention(): void {
  document.getElementById("attention")!.classList.remove("active");
}

function updateAttentionPanel(): void {
  if (!trace) return;
  const window = trace.summary.attention_windows.find(
    (candidate) => simTime >= candidate.from_seconds && simTime < candidate.to_seconds,
  );
  if (window) {
    const required = trace.events.find(
      (event) => event.event === "attention-required" && event.node === window.node,
    );
    showAttention(String(required?.prompt ?? window.node));
  } else {
    hideAttention();
  }
}

/** Seek: replay from zero, snapping. Play: continue from `applied`,
 * animating. */
function applyUpTo(t: number, animate: boolean): void {
  if (!trace) return;
  if (t < simTime || applied === 0) {
    resetState();
    animate = false;
  }
  simTime = t;
  while (applied < trace.events.length && trace.events[applied].t <= t) {
    applyEvent(trace.events[applied], animate);
    applied += 1;
  }
  updateAttentionPanel();
  updateHud();
}

// ---------- hud ----------

function hms(seconds: number): string {
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function updateHud(): void {
  if (!trace) return;
  document.getElementById("clock")!.textContent = `t+${hms(simTime)}`;
  const fraction = trace.summary.total_seconds > 0 ? simTime / trace.summary.total_seconds : 0;
  (document.getElementById("cursor") as HTMLElement).style.left =
    `${Math.min(100, fraction * 100)}%`;
}

function buildTimeline(): void {
  if (!trace) return;
  const timeline = document.getElementById("timeline")!;
  for (const window of trace.summary.attention_windows) {
    const span = document.createElement("div");
    span.className = "attention-span";
    const total = trace.summary.total_seconds;
    span.style.left = `${(window.from_seconds / total) * 100}%`;
    span.style.width = `${Math.max(0.4, ((window.to_seconds - window.from_seconds) / total) * 100)}%`;
    span.title = window.node;
    timeline.appendChild(span);
  }
  timeline.addEventListener("pointerdown", (pointer) => {
    const rect = timeline.getBoundingClientRect();
    const fraction = (pointer.clientX - rect.left) / rect.width;
    applyUpTo(Math.max(0, Math.min(1, fraction)) * trace!.summary.total_seconds, false);
  });
  const summary = trace.summary;
  document.getElementById("summary")!.innerHTML =
    `${trace!.summary.nodes} nodes, total ${hms(summary.total_seconds)}<br />` +
    `<span class="dim">attended ${hms(summary.attended_seconds)} in ` +
    `${summary.attention_windows.length} window(s); ` +
    `walk-away ${hms(summary.walkaway_seconds)}</span><br />` +
    `<span class="dim">durations: ${trace!.durations} (estimates)</span>`;
}

// ---------- renderer ----------

const app = document.getElementById("app")!;
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setPixelRatio(window.devicePixelRatio);
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.1;
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
app.appendChild(renderer.domElement);

const three = new THREE.Scene();
three.background = new THREE.Color(0x14161a);
// Image-based lighting from a procedural room: zero external assets.
const pmrem = new THREE.PMREMGenerator(renderer);
three.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
const camera = new THREE.PerspectiveCamera(
  50,
  window.innerWidth / window.innerHeight,
  0.01,
  100,
);
camera.position.set(1.4, 1.9, 1.5);
const controls = new OrbitControls(camera, renderer.domElement);
controls.target.set(0.9, 1.0, -0.3);

three.add(new THREE.AmbientLight(0xffffff, 0.6));
const key = new THREE.DirectionalLight(0xffffff, 1.6);
key.position.set(2, 4, 3);
key.castShadow = true;
key.shadow.mapSize.set(2048, 2048);
key.shadow.camera.left = -4;
key.shadow.camera.right = 4;
key.shadow.camera.top = 4;
key.shadow.camera.bottom = -4;
key.shadow.camera.far = 20;
key.shadow.bias = -0.0004;
three.add(key);
const fill = new THREE.DirectionalLight(0xffffff, 0.5);
fill.position.set(-3, 2, -2);
three.add(fill);

// Lab frame (Z-up millimeters) into three's Y-up meters.
const labRoot = new THREE.Group();
labRoot.rotation.x = -Math.PI / 2;
labRoot.scale.setScalar(0.001);
three.add(labRoot);

window.addEventListener("resize", () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
});

let lastFrame = performance.now();
function frame(now: number): void {
  const dt = (now - lastFrame) / 1000;
  lastFrame = now;
  if (playing && trace) {
    const next = Math.min(simTime + dt * speed, trace.summary.total_seconds);
    applyUpTo(next, true);
    if (next >= trace.summary.total_seconds) setPlaying(false);
  }

  // Cosmetic motion between true states: glide the pipetting heads to
  // their latest frame targets, ease labware tweens, pulse running
  // cyclers.
  const ease = 1 - Math.exp(-dt * 10);
  for (const [station, target] of headTargets) {
    const head = heads.get(station);
    if (head?.visible) head.position.lerp(target, ease);
  }
  tweens = tweens.filter((tween) => {
    const fraction = Math.min(1, (now - tween.started) / (TWEEN_SECONDS * 1000));
    const smooth = fraction * fraction * (3 - 2 * fraction);
    tween.object.position.lerpVectors(tween.from, tween.to, smooth);
    // Carry the plate in an arc rather than through the bench.
    tween.object.position.z += Math.sin(smooth * Math.PI) * 120;
    return fraction < 1;
  });
  for (const station of thermalActive) {
    const mesh = stationMeshes.get(station);
    if (mesh) {
      (mesh.material as THREE.MeshStandardMaterial).emissiveIntensity =
        1.0 + 0.8 * Math.sin(now / 250);
    }
  }

  controls.update();
  renderer.render(three, camera);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// ---------- transport controls ----------

function setPlaying(on: boolean): void {
  playing = on;
  const button = document.getElementById("play")!;
  button.textContent = on ? "pause" : "play";
  button.classList.toggle("on", on);
}

document.getElementById("play")!.addEventListener("click", () => {
  if (trace && simTime >= trace.summary.total_seconds) applyUpTo(0, false);
  setPlaying(!playing);
});
for (const button of document.querySelectorAll<HTMLButtonElement>("[data-speed]")) {
  button.addEventListener("click", () => {
    speed = Number(button.dataset.speed);
    for (const other of document.querySelectorAll("[data-speed]")) {
      other.classList.toggle("on", other === button);
    }
  });
}

// ---------- loading ----------

/** Fits the camera to whatever was just built: benches and whole rooms
 * both deserve a establishing shot. */
function frameScene(): void {
  const bounds = new THREE.Box3().setFromObject(labRoot);
  if (bounds.isEmpty()) return;
  const center = bounds.getCenter(new THREE.Vector3());
  const size = bounds.getSize(new THREE.Vector3());
  const radius = Math.max(size.x, size.y, size.z);
  controls.target.copy(center);
  camera.position.set(
    center.x + radius * 0.55,
    center.y + radius * 0.65,
    center.z + radius * 0.9,
  );
  camera.near = radius / 100;
  camera.far = radius * 20;
  camera.updateProjectionMatrix();
}

function tryStart(): void {
  if (!sceneDoc || !trace) return;
  labRoot.clear();
  byId.clear();
  stationMeshes.clear();
  labRoot.add(buildNode(sceneDoc.root));
  frameScene();
  rememberHomes();
  buildTimeline();
  applyUpTo(0, false);
  document.getElementById("drop")!.classList.add("hidden");
  document.getElementById("status")!.textContent =
    `${sceneDoc.name} · ${trace.events.length} events`;
  setPlaying(true);
}

function accept(name: string, parsed: unknown): void {
  const document_ = parsed as { format?: string };
  const status = document.getElementById("drop-status")!;
  if (document_.format === "lab.scene.v0") {
    sceneDoc = parsed as SceneDocument;
    status.textContent = trace ? "" : "scene loaded — now the sim-trace.json";
  } else if (document_.format === "lab.sim-trace.v0") {
    trace = parsed as TraceDocument;
    status.textContent = sceneDoc ? "" : "trace loaded — now the scene.json";
  } else {
    status.textContent = `${name}: not a lab.scene.v0 or lab.sim-trace.v0 document`;
    return;
  }
  tryStart();
}

document.addEventListener("dragover", (event) => event.preventDefault());
document.addEventListener("drop", async (event) => {
  event.preventDefault();
  for (const file of event.dataTransfer?.files ?? []) {
    try {
      accept(file.name, JSON.parse(await file.text()));
    } catch {
      document.getElementById("drop-status")!.textContent = `${file.name}: not JSON`;
    }
  }
});

// Served alongside its data (or with ?scene=&trace= query params), the
// player loads without any drop.
async function autoload(): Promise<void> {
  const parameters = new URLSearchParams(window.location.search);
  const sceneUrl = parameters.get("scene") ?? "scene.json";
  const traceUrl = parameters.get("trace") ?? "sim-trace.json";
  for (const [name, url] of [
    ["scene", sceneUrl],
    ["trace", traceUrl],
  ] as const) {
    try {
      const response = await fetch(url);
      if (response.ok) accept(name, await response.json());
    } catch {
      // Drag and drop remains the path.
    }
  }
}
void autoload();
