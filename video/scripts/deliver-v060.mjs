/**
 * Deliver the rendered v060 video to website/assets with content-hashed names.
 * Never overwrites existing files. Writes delivery-v060.json with verified metadata.
 *
 * Usage: node scripts/deliver-v060.mjs
 *   Run npm run verify-v060 first; this script imports it.
 */
import {execFileSync} from 'node:child_process';
import {readFile, stat, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';

// Validate the v060 manifest before delivering.
await import('./verify-v060.mjs');

const output = new URL('../out/secblitz-demo-v060.mp4', import.meta.url).pathname;
const assetsDir = new URL('../../website/assets/', import.meta.url).pathname;

// Compute content hash of the rendered output to derive the filename.
const mp4Bytes = await readFile(output);
assert.ok((await stat(output)).size < 10 * 1024 * 1024, 'Render exceeds the 10 MiB delivery target.');
const mp4Hash = createHash('sha256').update(mp4Bytes).digest('hex');
const mp4Prefix = mp4Hash.slice(0, 12);
const mp4Name = `intro-${mp4Prefix}.mp4`;
const webpName = `preview-${mp4Prefix}.webp`;
const mp4Dest = `${assetsDir}${mp4Name}`;
const webpDest = `${assetsDir}${webpName}`;

// Guard: refuse to overwrite the approved v031 delivery or any existing file.
const protected031 = ['intro-6bb434a9c067.mp4', 'preview-33b342ab21fb.webp'];
assert.ok(!protected031.includes(mp4Name), `Content hash collision with approved v031 delivery; hashes must differ.`);
assert.ok(!protected031.includes(webpName), `Content hash collision with approved v031 delivery; hashes must differ.`);

// -n refuses to overwrite existing files.
execFileSync('ffmpeg', ['-v', 'error', '-n', '-i', output,
  '-map', '0:v:0', '-c:v', 'copy', '-an', '-movflags', '+faststart', mp4Dest], {stdio: 'inherit'});

const manifest = JSON.parse(await readFile(new URL('../sources-v060.json', import.meta.url), 'utf8'));
// Poster: use posterFrame from manifest, decode from the delivered faststart file.
execFileSync('ffmpeg', ['-v', 'error', '-n', '-i', mp4Dest,
  '-vf', `select=eq(n\\,${manifest.posterFrame})`, '-frames:v', '1',
  '-c:v', 'libwebp', '-lossless', '1', webpDest], {stdio: 'inherit'});

// Probe and record delivered assets.
const records = [];
for (const [name, dest] of [[mp4Name, mp4Dest], [webpName, webpDest]]) {
  const bytes = await readFile(dest);
  const hash = createHash('sha256').update(bytes).digest('hex');
  // Verify content hash matches the filename prefix.
  assert.ok(hash.startsWith(name.split('-')[1].split('.')[0]), `Delivered bytes must match content-hashed filename: ${name}`);
  const probe = JSON.parse(execFileSync('ffprobe', ['-v', 'error', '-count_frames',
    '-show_streams', '-show_format', '-of', 'json', dest]));
  const stream = probe.streams[0];
  let atoms;
  if (name.endsWith('.mp4')) {
    assert.equal(stream.codec_name, 'h264');
    assert.equal(stream.pix_fmt, 'yuv420p');
    assert.equal(stream.avg_frame_rate, '30/1');
    assert.equal(probe.streams.length, 1);
    assert.ok(bytes.length < 10 * 1024 * 1024);
    atoms = [];
    for (let offset = 0; offset < bytes.length;) {
      const length = bytes.readUInt32BE(offset);
      atoms.push({type: bytes.toString('ascii', offset + 4, offset + 8), offset});
      assert.ok(length >= 8);
      offset += length;
    }
    assert.ok(atoms.find(a => a.type === 'moov').offset < atoms.find(a => a.type === 'mdat').offset, 'faststart required');
    execFileSync('ffmpeg', ['-v', 'error', '-i', dest, '-f', 'null', '-']);
  }
  records.push({
    path: `website/assets/${name}`,
    bytes: bytes.length,
    sha256: hash,
    codec: stream.codec_name,
    width: stream.width,
    height: stream.height,
    durationSeconds: name.endsWith('.mp4') ? Number(probe.format.duration) : null,
    frames: Number(stream.nb_read_frames),
    pixelFormat: stream.pix_fmt ?? null,
    atoms: atoms ?? undefined,
  });
}

const record = {
  framework: 'Remotion 4.0.532',
  renderCommand: 'TMPDIR="$PWD/out/render-temp" npm run render-v060 -- --overwrite --log=error',
  deliveryCommand: 'node scripts/deliver-v060.mjs',
  sourceManifest: 'video/sources-v060.json',
  sourceVersion: '0.6.0',
  closeFrames: 0,
  visualReview: 'PENDING: reviewer must verify home screen logo, scan checklist, report risk lines, payoff list, undo result.',
  assets: records,
};
await writeFile(new URL('../delivery-v060.json', import.meta.url), JSON.stringify(record, null, 2) + '\n');
console.log(`Delivered v060: ${mp4Name} and ${webpName}. Update website/index.html references to new filenames.`);
console.log(JSON.stringify(record, null, 2));
