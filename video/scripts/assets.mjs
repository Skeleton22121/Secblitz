import {copyFile, mkdir} from 'node:fs/promises';

await mkdir(new URL('../public/brand/', import.meta.url), {recursive: true});
for (const [source, target] of [
  ['secblitz.svg', 'secblitz.svg'],
  ['fonts/schibsted-grotesk-latin.woff2', 'schibsted-grotesk-latin.woff2'],
]) {
  await copyFile(new URL(`../../website/assets/${source}`, import.meta.url),
    new URL(`../public/brand/${target}`, import.meta.url));
}
console.log('Approved website logo and font copied into video/public/brand.');
