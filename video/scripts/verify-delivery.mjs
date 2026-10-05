import {readFile, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';

const records = [];
for (const name of ['intro-6bb434a9c067.mp4', 'preview-33b342ab21fb.webp']) {
  const path = new URL(`../../website/assets/${name}`, import.meta.url).pathname;
  const bytes = await readFile(path);
  assert.ok(createHash('sha256').update(bytes).digest('hex').startsWith(name.split('-')[1].split('.')[0]), 'Delivered bytes must match the content-hashed filename.');
  const probe = JSON.parse(execFileSync('ffprobe', ['-v', 'error', '-count_frames',
    '-show_streams', '-show_format', '-of', 'json', path]));
  const stream = probe.streams[0];
  assert.equal(probe.streams.length, 1);
  assert.equal(stream.width, 1280);
  assert.equal(stream.height, 720);
  let atoms;
  if (name.endsWith('.mp4')) {
    assert.equal(stream.codec_name, 'h264');
    assert.equal(stream.pix_fmt, 'yuv420p');
    assert.equal(stream.avg_frame_rate, '30/1');
    assert.equal(Number(stream.nb_read_frames), 990);
    assert.equal(Number(probe.format.duration), 33);
    assert.ok(bytes.length < 10 * 1024 * 1024);
    atoms = [];
    for (let offset = 0; offset < bytes.length;) {
      const length = bytes.readUInt32BE(offset);
      atoms.push({type: bytes.toString('ascii', offset + 4, offset + 8), offset});
      assert.ok(length >= 8);
      offset += length;
    }
    assert.ok(atoms.find(a => a.type === 'moov').offset < atoms.find(a => a.type === 'mdat').offset);
    execFileSync('ffmpeg', ['-v', 'error', '-i', path, '-f', 'null', '-']);
  } else assert.equal(stream.codec_name, 'webp');
  records.push({path: `website/assets/${name}`, bytes: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    codec: stream.codec_name, width: stream.width, height: stream.height,
    durationSeconds: name.endsWith('.mp4') ? Number(probe.format.duration) : null,
    frames: Number(stream.nb_read_frames), pixelFormat: stream.pix_fmt, atoms});
}
const record = {
  framework: 'Remotion 4.0.532', renderCommand: 'npm run render', deliveryCommand: 'npm run deliver',
  sourceManifest: 'video/sources.json', sourceVersion: '0.3.1',
  visualReview: 'Reviewed raw decoded frames, Remotion stills, final MP4 one-second contact sheet and confirmation/undo/close samples. Public firewall selection, Fixed row and restored original setting correspond. Actual footage occupies 31 of 33 seconds. No audio stream. Poster is exact decoded first frame encoded losslessly.',
  assets: records,
};
await writeFile(new URL('../delivery.json', import.meta.url), JSON.stringify(record, null, 2) + '\n');
console.log(JSON.stringify(record, null, 2));
