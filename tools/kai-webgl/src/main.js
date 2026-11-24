import { createGraph } from './scene.js';
import { framesFromTrace } from './trace.js';

const $ = selector => document.querySelector(selector);
const play = $('#play');
const step = $('#step');
const open = $('#open-trace');
const file = $('#trace-file');
const error = $('#error');
const checkpoint = $('#checkpoint');
const viewport = $('.viewport');

let frames = [];
let current = 0;
let playing = false;
let nextFrameAt = 0;
let graph;

function playback(on) {
  playing = on;
  nextFrameAt = performance.now() + 480;
  play.textContent = playing ? 'Pause' : current === frames.length - 1 ? 'Replay' : 'Play';
}

function show(index) {
  current = index;
  const frame = frames[index];
  graph.show(frame);
  step.value = index + 1;
  $('#position').textContent = (index + 1) + ' / ' + frames.length;
  $('#phase').textContent = frame.phase === 'boot' ? 'Boot stages' : 'Saved program';
  $('#message').textContent = frame.message;
  viewport.dataset.blocked = frame.blocked;
  checkpoint.hidden = !frame.checkpoint;
  if (frame.checkpoint) {
    $('#generation').textContent = frame.checkpoint.generation;
    $('#value').textContent = frame.checkpoint.x;
    $('#address').textContent = frame.checkpoint.cursor;
  }
}

function useTrace(text, source) {
  frames = framesFromTrace(text);
  step.max = frames.length;
  step.disabled = frames.length < 2;
  play.disabled = frames.length < 2;
  $('#source').textContent = source;
  error.hidden = true;
  show(0);
  playback(frames.length > 1 && !matchMedia('(prefers-reduced-motion: reduce)').matches);
}

function animate(now) {
  if (playing && now >= nextFrameAt) {
    show(current + 1);
    playback(current < frames.length - 1);
  }
  graph.draw();
  requestAnimationFrame(animate);
}

function showError(reason) {
  error.textContent = reason.message;
  error.hidden = false;
}

open.disabled = true;
try {
  graph = createGraph($('#graph'), $('#node-labels'));
  $('#renderer').textContent = graph.description;
  const response = await fetch('/fixtures/recovered.log');
  if (!response.ok) throw new Error('The QEMU recovery trace could not be loaded.');
  useTrace(await response.text(), 'QEMU recovery trace');
  open.disabled = false;
  requestAnimationFrame(animate);
} catch (reason) {
  $('#phase').textContent = 'Preview unavailable';
  showError(reason);
}

play.addEventListener('click', () => {
  if (playing) {
    playback(false);
  } else {
    if (current === frames.length - 1) show(0);
    playback(true);
  }
});
step.addEventListener('input', () => {
  show(Number(step.value) - 1);
  playback(false);
});
open.addEventListener('click', () => file.click());
file.addEventListener('change', async () => {
  const selected = file.files[0];
  file.value = '';
  if (!selected) return;
  try {
    useTrace(await selected.text(), selected.name);
  } catch (reason) {
    showError(reason);
  }
});
