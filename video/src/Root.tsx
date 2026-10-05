import {Composition} from 'remotion';
import manifestJson from '../sources.json';
import manifestV060Json from '../sources-v060.json';
import type {Manifest} from './manifest';
import {Demo} from './Demo';
import {totalFrames} from './timing.mjs';

const manifest = manifestJson as Manifest;
const manifestV060 = manifestV060Json as Manifest;

export const Root = () => <>
  <Composition
    id="SecblitzDemo"
    component={Demo}
    width={manifest.width}
    height={manifest.height}
    fps={manifest.fps}
    durationInFrames={manifest.segments.length ? totalFrames(manifest) : 840}
    defaultProps={{manifest}}
  />
  <Composition
    id="SecblitzDemoV060"
    component={Demo}
    width={manifestV060.width}
    height={manifestV060.height}
    fps={manifestV060.fps}
    durationInFrames={manifestV060.segments.length ? totalFrames(manifestV060) : 840}
    defaultProps={{manifest: manifestV060}}
  />
</>;
