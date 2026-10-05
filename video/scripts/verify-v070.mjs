import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {segmentFrames, totalFrames} from '../src/timing.mjs';

// 0.7.0 website loop: 20 s, no text, no outro, last frame continues into the first.
const m = JSON.parse(await readFile(new URL('../sources-v070.json', import.meta.url), 'utf8'));
assert.equal(m.status, 'ready', 'Capture handoff is pending.');
assert.equal(m.fps, 30);
assert.equal(m.width, 1920);
assert.equal(m.height, 1080);
assert.equal(m.closeFrames, 0, 'The loop has no outro.');
assert.equal(m.fixturesUsed, false);
for (const key of ['captureMethod', 'versionEvidence', 'originalRecordingPath'])
  assert.ok(m.provenance[key], `Missing provenance: ${key}`);
assert.ok(m.sources.length && m.segments.length);
for (const source of m.sources) {
  assert.match(source.file, /^capture\/[a-zA-Z0-9_.-]+$/);
  const file = new URL(`../public/${source.file}`, import.meta.url);
  const hash = createHash('sha256').update(await readFile(file)).digest('hex');
  assert.equal(hash, source.sha256, `Hash mismatch: ${source.id}`);
  const probe = JSON.parse(execFileSync('ffprobe', ['-v', 'error', '-show_streams', '-show_format', '-of', 'json', file.pathname]));
  const video = probe.streams.find(s => s.codec_type === 'video');
  assert.equal(video.width, source.width);
  assert.equal(video.height, source.height);
  assert.ok(Math.abs(Number(probe.format.duration) - source.durationSeconds) < 0.1);
}
// Take D 229 to 340 s shows the password generator; it must never be used.
const forbidden = {sourceId: 'd', from: 229, to: 340};
let cursor = 0;
for (const s of m.segments) {
  const source = m.sources.find(item => item.id === s.sourceId);
  assert.ok(source, `Missing source for ${s.id}`);
  assert.ok(s.sourceInSeconds >= 0 && s.sourceOutSeconds > s.sourceInSeconds);
  assert.ok(s.sourceOutSeconds <= source.durationSeconds);
  assert.ok(s.playbackRate >= 1 && s.playbackRate <= 16);
  assert.equal(s.outputFromFrame, cursor, `Gap or overlap before ${s.id}`);
  assert.equal(s.outputDurationFrames, segmentFrames(s, m.fps));
  assert.ok(s.outputDurationFrames > 0 && s.evidence);
  if (s.sourceId === forbidden.sourceId)
    assert.ok(s.sourceOutSeconds <= forbidden.from || s.sourceInSeconds >= forbidden.to,
      `${s.id} uses footage that shows the password generator.`);
  for (const c of [s.camera.from, s.camera.to]) {
    assert.ok(c.zoom >= 1 && c.zoom <= 2.5, `${s.id}: zoom out of range`);
    assert.ok(c.x >= 0 && c.x <= source.width && c.y >= 0 && c.y <= source.height, `${s.id}: camera off the recording`);
  }
  cursor += s.outputDurationFrames;
}
assert.equal(totalFrames(m), 600, 'The loop must be exactly 20 seconds.');
// Seamless: the last cut ends on the source frame right before the first cut
// starts, with the same camera.
const first = m.segments[0];
const last = m.segments.at(-1);
assert.equal(last.sourceId, first.sourceId, 'Loop must close on the take it opens with.');
assert.equal(Math.round(last.sourceOutSeconds * m.fps), Math.round(first.sourceInSeconds * m.fps), 'Loop seam must be continuous.');
assert.equal(first.playbackRate, last.playbackRate);
assert.deepEqual(last.camera.to, first.camera.from, 'Loop seam camera must match.');
assert.ok(Number.isInteger(m.posterFrame) && m.posterFrame >= 0 && m.posterFrame < totalFrames(m));
console.log(`Verified hashes, gap-free timeline and seamless loop: ${totalFrames(m)} frames, ${totalFrames(m) / m.fps}s.`);
