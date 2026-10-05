import {execFileSync} from 'node:child_process';
import {readFile, stat} from 'node:fs/promises';
import assert from 'node:assert/strict';

await import('./verify.mjs');
const manifest = JSON.parse(await readFile(new URL('../sources.json', import.meta.url), 'utf8'));
const output = new URL('../out/secblitz-demo.mp4', import.meta.url).pathname;
const destination = new URL('../../website/assets/intro-6bb434a9c067.mp4', import.meta.url).pathname;
assert.ok((await stat(output)).size < 10 * 1024 * 1024, 'Render exceeds the 10 MiB delivery target.');
if (!process.argv.includes('--poster-only')) {
  execFileSync('ffmpeg', ['-n', '-i', output, '-map', '0:v:0', '-c:v', 'copy', '-an', '-movflags', '+faststart', destination], {stdio: 'inherit'});
}
// Decode the exact delivered frame, then encode a lossless WebP poster.
execFileSync('ffmpeg', ['-v', 'error', '-n', '-i', destination,
  '-vf', `select=eq(n\\,${manifest.posterFrame})`, '-frames:v', '1',
  '-c:v', 'libwebp', '-lossless', '1',
   new URL('../../website/assets/preview-33b342ab21fb.webp', import.meta.url).pathname], {stdio: 'inherit'});
console.log('Delivered silent MP4 with faststart and a real-footage poster.');
