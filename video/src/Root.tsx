import {Composition} from 'remotion';
import manifestJson from '../sources.json';
import manifestV060Json from '../sources-v060.json';
import manifestV070Json from '../sources-v070.json';
import type {LoopManifest, Manifest} from './manifest';
import {Demo} from './Demo';
import {Loop} from './Loop';
import {totalFrames} from './timing.mjs';

const manifest = manifestJson as Manifest;
const manifestV060 = manifestV060Json as Manifest;
const manifestV070 = manifestV070Json as LoopManifest;

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
  <Composition
    id="SecblitzLoop070"
    component={Loop}
    width={manifestV070.width}
    height={manifestV070.height}
    fps={manifestV070.fps}
    durationInFrames={manifestV070.segments.length ? totalFrames(manifestV070) : 600}
    defaultProps={{manifest: manifestV070}}
  />
</>;
