import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { framesFromTrace } from '../src/trace.js';

const trace = await readFile(new URL('../fixtures/recovered.log', import.meta.url), 'utf8');

test('replays the captured nodes, links and restored checkpoints in order', () => {
  const frames = framesFromTrace(trace);
  const visible = frames.map(frame => [frame.phase, frame.nodes.length, frame.links.length]);
  assert.deepEqual(visible.slice(0, 11), [
    ['boot', 1, 0], ['boot', 2, 0], ['boot', 3, 0], ['boot', 4, 0], ['boot', 4, 3],
    ['program', 1, 0], ['program', 2, 0], ['program', 3, 0], ['program', 4, 0],
    ['program', 5, 0], ['program', 5, 6],
  ]);
  assert.deepEqual(frames[10].links, [
    ['0', '1'], ['0', '4'], ['1', '2'], ['2', '3'], ['3', '1'], ['4', '1'],
  ]);
  assert.deepEqual(frames.slice(11).map(({ checkpoint, focus }) => [checkpoint.x, focus]), [
    ['3', '1'], ['5', '3'], ['7', '2'], ['9', '1'],
  ]);
  assert.equal(frames[11].message, 'Checkpoint restored');
  assert.equal(frames.at(-1).checkpoint.status, 'finished');
});

test('shows an unavailable storage node without inventing a program', () => {
  const log = [
    'PYTHOS:VAULT_VIEW:NODE CORE',
    'PYTHOS:VAULT_VIEW:NODE STORAGE',
    'PYTHOS:VAULT_VIEW:BOOT_LINKS',
    'PYTHOS:VAULT_VIEW:STOP VIRTIO_BLOCK',
  ].join('\r\n');
  const frame = framesFromTrace(log).at(-1);
  assert.equal(frame.phase, 'boot');
  assert.equal(frame.checkpoint, null);
  assert.equal(frame.focus, 'STORAGE');
  assert.equal(frame.message, 'Virtio test disk unavailable');
  assert.equal(frame.blocked, true);
});

test('preserves a 64-bit checkpoint value exactly', () => {
  const log = [
    'PYTHOS:VAULT_VIEW:ADDRESS 0',
    'PYTHOS:VAULT_VIEW:LINK 0 0',
    'PYTHOS:VAULT_VIEW:STATE pulse generation:18446744073709551615 x:18446744073709551614 cursor:0 status:finished',
  ].join('\n');
  const { checkpoint } = framesFromTrace(log).at(-1);
  assert.equal(checkpoint.generation, '18446744073709551615');
  assert.equal(checkpoint.x, '18446744073709551614');
});

test('rejects a log link to an instruction that was never revealed', () => {
  const log = 'PYTHOS:VAULT_VIEW:ADDRESS 0\nPYTHOS:VAULT_VIEW:LINK 0 1';
  assert.throws(() => framesFromTrace(log), /unseen instruction/);
});
