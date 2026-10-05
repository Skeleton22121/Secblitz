import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {segmentFrames, totalFrames, footageFrames} from '../src/timing.mjs';

const m = JSON.parse(await readFile(new URL('../sources-v060.json', import.meta.url), 'utf8'));
assert.equal(m.status, 'ready', 'Capture handoff is pending.');
assert.equal(m.fps, 30);
assert.equal(m.width, 1280);
assert.equal(m.height, 720);
// closeFrames 0 is valid for v060: the edit ends on real footage with no brand close.
assert.ok(Number.isInteger(m.closeFrames) && m.closeFrames >= 0);
assert.equal(m.closeFrames, 0, 'v060 has no outro; closeFrames must be 0.');
assert.equal(typeof m.fixturesUsed, 'boolean');
for (const key of ['captureMethod', 'versionEvidence', 'originalRecordingPath'])
  assert.ok(m.provenance[key], `Missing provenance: ${key}`);
if (m.fixturesUsed) assert.ok(m.provenance.fixtureDetails);
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
let cursor = 0;
let previousSourceEnd = 0;
let previousSourceId = null;
for (const s of m.segments) {
  const source = m.sources.find(item => item.id === s.sourceId);
  assert.ok(source, `Missing source for ${s.id}`);
  assert.ok(s.sourceInSeconds >= 0 && s.sourceOutSeconds > s.sourceInSeconds);
  assert.ok(s.sourceOutSeconds <= source.durationSeconds);
  assert.ok(s.playbackRate >= 1 && s.playbackRate <= 10);
  assert.equal(s.outputFromFrame, cursor, `Gap or overlap before ${s.id}`);
  assert.equal(s.outputDurationFrames, segmentFrames(s, m.fps));
  assert.ok(s.outputDurationFrames > 0 && s.evidence);
  assert.ok(['check', 'choose', 'fix', 'undo', 'tools'].includes(s.phase));
  if (s.sourceId === previousSourceId) assert.ok(s.sourceInSeconds >= previousSourceEnd, 'Source chronology reversed.');
  previousSourceId = s.sourceId;
  previousSourceEnd = s.sourceOutSeconds;
  if (s.crop) {
    const c = s.crop;
    assert.ok(c.x >= 0 && c.y >= 0 && c.width > 0 && c.height > 0);
    assert.ok(c.x + c.width <= source.width && c.y + c.height <= source.height);
  }
  cursor += s.outputDurationFrames;
}
// v060: no outro, so total frames = footage frames only
assert.ok(totalFrames(m) >= 750 && totalFrames(m) <= 1050, 'Edit must be 25 to 35 seconds.');
assert.ok(Number.isInteger(m.posterFrame) && m.posterFrame >= 0 && m.posterFrame < footageFrames(m));
console.log(`Verified hashes, source bounds and gap-free timeline: ${totalFrames(m)} frames, ${totalFrames(m) / m.fps}s (no outro).`);
