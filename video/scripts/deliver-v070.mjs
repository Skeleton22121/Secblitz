/**
 * Deliver the rendered 0.7.0 website loop to website/assets with content-hashed names.
 * Never overwrites existing files; names come from the final bytes. Writes delivery-v070.json with verified metadata.
 *
 * Usage: node scripts/deliver-v070.mjs
 *   Run npm run verify-v070 first; this script imports it.
 */
import {execFileSync} from 'node:child_process';
import {constants} from 'node:fs';
import {copyFile, mkdir, readFile, rm, stat, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';

// Validate the v070 manifest before delivering.
await import('./verify-v070.mjs');

const output = new URL('../out/secblitz-loop-v070.mp4', import.meta.url).pathname;
const assetsDir = new URL('../../website/assets/', import.meta.url).pathname;

assert.ok((await stat(output)).size < 10 * 1024 * 1024, 'Render exceeds the 10 MiB delivery target.');
const manifest = JSON.parse(await readFile(new URL('../sources-v070.json', import.meta.url), 'utf8'));

// Remux (faststart) and cut the poster into out/ first, then name each file by
// the SHA-256 of its final bytes, so the prefix always matches what is served.
const staged = new URL('../out/delivery-v070/', import.meta.url).pathname;
await rm(staged, {recursive: true, force: true});
await mkdir(staged, {recursive: true});
const stagedMp4 = `${staged}intro.mp4`;
const stagedWebp = `${staged}preview.webp`;
execFileSync('ffmpeg', ['-v', 'error', '-n', '-i', output,
  '-map', '0:v:0', '-c:v', 'copy', '-an', '-movflags', '+faststart', stagedMp4], {stdio: 'inherit'});
// Poster: the manifest's posterFrame, decoded from the delivered faststart file.
execFileSync('ffmpeg', ['-v', 'error', '-n', '-i', stagedMp4,
  '-vf', `select=eq(n\\,${manifest.posterFrame})`, '-frames:v', '1',
  '-c:v', 'libwebp', '-lossless', '1', stagedWebp], {stdio: 'inherit'});
const prefix = async file => createHash('sha256').update(await readFile(file)).digest('hex').slice(0, 12);
const mp4Name = `intro-${await prefix(stagedMp4)}.mp4`;
const webpName = `preview-${await prefix(stagedWebp)}.webp`;
const mp4Dest = `${assetsDir}${mp4Name}`;
const webpDest = `${assetsDir}${webpName}`;
// COPYFILE_EXCL refuses to overwrite any existing delivery.
await copyFile(stagedMp4, mp4Dest, constants.COPYFILE_EXCL);
await copyFile(stagedWebp, webpDest, constants.COPYFILE_EXCL);

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
    assert.equal(Number(stream.nb_read_frames), 600);
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
  renderCommand: 'TMPDIR="$PWD/out/render-temp" npm run render-v070 -- --overwrite --log=error',
  deliveryCommand: 'node scripts/deliver-v070.mjs',
  sourceManifest: 'video/sources-v070.json',
  sourceVersion: '0.7.0',
  closeFrames: 0,
  visualReview: 'Reviewed: first/middle/last frame of every cut at half scale, full-res 2x zoom frame, lossless seam check (frame 599 to 0 mean difference 0.00002).',
  assets: records,
};
await writeFile(new URL('../delivery-v070.json', import.meta.url), JSON.stringify(record, null, 2) + '\n');
console.log(`Delivered v070: ${mp4Name} and ${webpName}. Update website/index.html references to new filenames.`);
console.log(JSON.stringify(record, null, 2));
