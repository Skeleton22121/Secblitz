/**
 * Build sources-v070.json for the 0.7.0 website loop from the cut list below.
 * Times are seconds in the original takes; cameras are in source pixels of
 * the cropped client area (1424x802): {x, y} is the centre, zoom 1 = whole app.
 *
 * Usage: node scripts/plan-v070.mjs
 */
import {writeFile} from 'node:fs/promises';
import {segmentFrames} from '../src/timing.mjs';

const fps = 30;
const full = {x: 712, y: 401, zoom: 1};
const cam = (x, y, zoom) => ({x, y, zoom});

// [id, source, in, out, rate, from, to, evidence]
const cuts = [
  ['home-light-in', 'd', 458.0, 458.5, 1, full, full,
    'Home in light mode after the theme switch, 37 of 54 protected. Same frames that end the loop.'],
  ['check', 'c', 100.0, 113.5, 9, cam(712, 330, 1.15), cam(712, 300, 1.5),
    'Checking your PC: the radar scan runs through the protections.'],
  ['home-33', 'c', 133.0, 134.2, 1, cam(828, 180, 1.6), cam(828, 210, 1.35),
    'Scan done: Home shows 33 of 54 protected and offers to fix 4 things.'],
  ['fix-sheet', 'c', 142.5, 143.5, 1, cam(712, 409, 1.35), cam(712, 409, 1.5),
    'Fix 4 for me opens a sheet that lists exactly what will change before anything happens.'],
  ['fixing', 'c', 150.5, 170.0, 13, cam(712, 400, 1.6), cam(712, 400, 1.75),
    'Fixing your PC: each fix is ticked off as it is applied.'],
  ['fixed', 'c', 255.7, 257.1, 1, cam(712, 470, 1.25), cam(712, 450, 1.35),
    'Result: You are now more protected, with the list of what changed.'],
  ['home-37', 'c', 265.4, 266.6, 1, cam(828, 180, 1.5), full,
    'Back on Home the score is now 37 of 54.'],
  ['remove-sheet', 'd', 144.0, 145.4, 1, cam(712, 401, 1.2), cam(712, 401, 1.5),
    'Clean up apps: removing Weather, Secblitz says it keeps a copy that works offline.'],
  ['saving-copy', 'd', 157.1, 158.3, 1, cam(712, 401, 1.9), cam(712, 401, 2),
    'Removing apps: Weather shows Saving a copy before it is removed.'],
  ['removed', 'd', 167.2, 168.4, 1, cam(712, 401, 1.8), cam(712, 401, 1.6),
    'Result: 1 app removed.'],
  ['restoring', 'd', 202.2, 203.4, 1, cam(830, 235, 1.45), cam(830, 235, 1.55),
    'Removed apps: Weather is being brought back from the saved copy.'],
  ['restored', 'd', 210.4, 211.6, 1, cam(714, 700, 1.6), cam(714, 700, 1.75),
    'Weather is back on the PC.'],
  ['tools', 'd', 340.4, 342.4, 2, cam(900, 640, 1.25), cam(900, 600, 1.4),
    'Tools: PC health tips found 5 things worth a look.'],
  ['dark-switch', 'd', 389.6, 390.6, 1, cam(1222, 172, 2.2), cam(1150, 250, 1.4),
    'Settings: the theme switches to Dark.'],
  ['home-dark', 'd', 397.6, 398.6, 1, cam(712, 300, 1.2), full,
    'Home in dark mode.'],
  ['light-switch', 'd', 443.7, 444.5, 1, cam(1222, 172, 2.2), cam(1150, 250, 1.6),
    'Settings: the theme switches back to Light.'],
  ['home-light-out', 'd', 456.3, 458.0, 1, cam(712, 300, 1.3), full,
    'Home in light mode; the loop continues from this frame.'],
];

const sources = {
  c: {id: 'c', file: 'capture/v070-take-c.mp4',
    sha256: 'bf59a74e563de0710f5f37ef2d08016fd02386cd71160ba9e416fe8817dda181',
    durationSeconds: 366.6, width: 1424, height: 802},
  d: {id: 'd', file: 'capture/v070-take-d.mp4',
    sha256: 'f74240d431fd7136f10d8c6a34744a729795d521d797b2008cf1e78b1c2d4847',
    durationSeconds: 560.066667, width: 1424, height: 802},
};

let cursor = 0;
const segments = cuts.map(([id, sourceId, sourceInSeconds, sourceOutSeconds, playbackRate, from, to, evidence]) => {
  const segment = {id, sourceId, sourceInSeconds, sourceOutSeconds, playbackRate,
    outputFromFrame: cursor, outputDurationFrames: 0, camera: {from, to}, evidence};
  segment.outputDurationFrames = segmentFrames(segment, fps);
  cursor += segment.outputDurationFrames;
  return segment;
});

const manifest = {
  status: 'ready',
  fps,
  width: 1920,
  height: 1080,
  closeFrames: 0,
  posterFrame: 0,
  fixturesUsed: false,
  provenance: {
    productVersion: '0.7.0',
    environment: 'Secblitz-W11-UI-Test',
    captureMethod: 'FFmpeg gdigrab of the visible desktop at 30 fps, no cursor, libx264 crf 12 yuv444p; '
      + 'then cropped to the Secblitz client area (1424x802 at 248,91) with timestamps kept, crf 10 yuv420p.',
    versionEvidence: 'Sidebar footer reads Version 0.7.0 in every source frame used.',
    originalRecordingPath: 'C:\\Users\\Public\\take070-c.mkv (sha256 ba5401881b9bd0a8120f89d9892ad318b8d77c6eec6769f775a9cd5d836b0af3), '
      + 'C:\\Users\\Public\\take070-d.mkv (sha256 fefcc0ac727bcbf2049ea556b33ab1820c05500d6be4c1d346da11581d6c7e3b)',
    fixtureDetails: null,
    exclusions: 'Tools frames that show the password generator (take D 229 to 340 s) are not used.',
  },
  sources: [sources.c, sources.d],
  segments,
};

await writeFile(new URL('../sources-v070.json', import.meta.url), JSON.stringify(manifest, null, 2) + '\n');
console.log(`sources-v070.json: ${segments.length} segments, ${cursor} frames (${(cursor / fps).toFixed(2)} s)`);
