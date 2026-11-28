import * as THREE from 'three';

const BOOT = {
  CORE: [-2.9, 0.9, 0.55],
  WORKER: [-1.1, -1.15, -0.65],
  MEMORY: [1.1, 0.75, -0.35],
  VAULT: [2.9, -0.75, 0.75],
  STORAGE: [2.9, -0.75, 0.75],
};
const PROGRAM = [
  [-2.4, .65, -.55], [-.75, -1.25, .65], [1.45, .68, .18], [2.85, -1.02, -.5],
  [-1.7, 1.75, -.9], [.45, 1.65, .01], [1.05, -1.9, -.98], [-2.8, -1.35, .12],
];

export function createGraph(canvas, labelLayer) {
  const context = canvas.getContext('webgl2', { alpha: true, antialias: true });
  if (!context) throw new Error('This browser did not provide a WebGL 2 context.');

  const renderer = new THREE.WebGLRenderer({ canvas, context, alpha: true, antialias: true });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  const scene = new THREE.Scene();
  const graph = new THREE.Group();
  scene.add(graph, new THREE.HemisphereLight(0xd3eaff, 0x334d5e, 2.6));

  const camera = new THREE.PerspectiveCamera(40, 1, .1, 60);
  camera.position.set(0, .3, 10);
  const focus = new THREE.Vector3();
  const cameraTarget = new THREE.Vector3();
  const projected = new THREE.Vector3();
  const eye = new THREE.Vector3();

  const ball = new THREE.SphereGeometry(.13, 24, 16);
  const ring = new THREE.RingGeometry(.23, .252, 36);
  const baseMaterial = new THREE.MeshStandardMaterial({ color: 0x83bddd, roughness: .65 });
  const strokes = new THREE.LineBasicMaterial({ color: 0x5e829b, transparent: true, opacity: .75 });
  const tones = [0x86f8d5, 0xffc189].map(color => ({
    node: new THREE.MeshStandardMaterial({ color, emissive: color, emissiveIntensity: .35, roughness: .5 }),
    halo: new THREE.MeshBasicMaterial({ color, transparent: true, opacity: .6, side: THREE.DoubleSide }),
  }));

  let labels = [];
  let edges = null;
  let halo = null;
  let width = 0;
  let height = 0;

  function show(frame) {
    graph.clear();
    edges?.dispose();
    labelLayer.replaceChildren();
    const points = new Map();

    labels = frame.nodes.map(({ id, name }) => {
      const point = new THREE.Vector3(...(frame.phase === 'boot' ? BOOT[id] : PROGRAM[id]));
      points.set(id, point);
      const isFocus = id === frame.focus;
      const tone = tones[Number(frame.blocked)];
      const node = new THREE.Mesh(ball, isFocus ? tone.node : baseMaterial);
      node.position.copy(point);
      graph.add(node);
      if (isFocus) {
        node.scale.setScalar(1.35);
        focus.copy(point);
        halo = new THREE.Mesh(ring, tone.halo);
        halo.position.copy(point);
        graph.add(halo);
      }

      const label = document.createElement('span');
      label.className = 'node-label';
      label.classList.toggle('focus', isFocus);
      label.classList.toggle('blocked', isFocus && frame.blocked);
      label.textContent = name;
      labelLayer.append(label);
      return { label, point };
    });

    const segments = [];
    for (const [from, to] of frame.links) {
      const start = points.get(from);
      const end = points.get(to);
      if (from === to) {
        const loop = new THREE.EllipseCurve(start.x + .27, start.y + .24, .26, .26).getPoints(28);
        for (let index = 1; index < loop.length; index += 1) {
          segments.push(new THREE.Vector3(loop[index - 1].x, loop[index - 1].y, start.z),
            new THREE.Vector3(loop[index].x, loop[index].y, start.z));
        }
      } else {
        const direction = end.clone().sub(start).normalize();
        const tip = end.clone().addScaledVector(direction, -.24);
        const tail = tip.clone().addScaledVector(direction, -.16);
        const wing = new THREE.Vector3(-direction.y, direction.x, 0).normalize().multiplyScalar(.06);
        segments.push(start, end, tip, tail.clone().add(wing), tip, tail.clone().sub(wing));
      }
    }
    edges = new THREE.BufferGeometry().setFromPoints(segments);
    graph.add(new THREE.LineSegments(edges, strokes));
  }

  function draw() {
    const nextWidth = canvas.clientWidth;
    const nextHeight = canvas.clientHeight;
    if (!nextWidth || !nextHeight) return;
    if (nextWidth !== width || nextHeight !== height) {
      width = nextWidth;
      height = nextHeight;
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    }
    eye.set(focus.x * .17, .3 + focus.y * .05, 8.7 * Math.max(1, 1.32 / camera.aspect));
    cameraTarget.set(focus.x * .09, focus.y * .08, 0);
    camera.position.lerp(eye, .11);
    camera.lookAt(cameraTarget);
    halo?.quaternion.copy(camera.quaternion);
    renderer.render(scene, camera);

    for (const { label, point } of labels) {
      projected.copy(point).project(camera);
      const center = (projected.x + 1) * width / 2;
      const half = label.offsetWidth / 2 + 2;
      label.style.left = Math.min(width - half, Math.max(half, center)) + 'px';
      label.style.top = (1 - projected.y) * height / 2 + 'px';
    }
  }

  return { show, draw, description: 'WebGL 2 · Three.js r' + THREE.REVISION };
}
