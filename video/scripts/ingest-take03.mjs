import {copyFile, readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';

const name = 'secblitz-v0.3.1-desktop-take03.webm';
const source = `/tmp/opencode/secblitz-v031-footage/${name}`;
assert.equal(createHash('sha256').update(await readFile(source)).digest('hex'),
  '4a17039c9cfc95572850b8b891575bfb74675bcd4874be1ee2c89d65308123d5');
await copyFile(source, new URL(`../public/capture/${name}`, import.meta.url));
console.log('Copied and SHA-256 verified Take 03 only.');
