import {loadFont} from '@remotion/fonts';
import {
  AbsoluteFill, Img, OffthreadVideo, Sequence, interpolate,
  spring, staticFile, useCurrentFrame, useRemotionEnvironment, useVideoConfig,
} from 'remotion';
import type {Manifest, Segment, Source} from './manifest';
import {footageFrames} from './timing.mjs';

loadFont({family: 'Schibsted', url: staticFile('brand/schibsted-grotesk-latin.woff2'), weight: '100 900'});

const color = {paper: '#f3f6f4', ink: '#0f2526', green: '#0a7a63', mint: '#9afad3'};
const titles = {
  check: 'Check your PC', choose: 'Choose your fixes',
  fix: 'Fix only what you pick', undo: 'Undo in a moment', tools: 'Check your PC',
};
const clamp = {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'} as const;

const Capture = ({segment, source}: {segment: Segment; source: Source}) => {
  const {fps} = useVideoConfig();
  const crop = segment.crop ?? {x: 0, y: 0, width: source.width, height: source.height};
  // Fit the documented source crop without stretching or overlaying the recorded UI.
  const scale = Math.min(1216 / crop.width, 566 / crop.height);
  return <div style={{position: 'absolute', top: 100, left: 32, width: 1216, height: 566,
    display: 'flex', alignItems: 'center', justifyContent: 'center'}}>
    <div style={{position: 'relative', width: crop.width * scale, height: crop.height * scale,
      overflow: 'hidden', borderRadius: 10, boxShadow: '0 10px 24px #0f252618'}}>
      <OffthreadVideo muted src={staticFile(source.file)}
        trimBefore={Math.round(segment.sourceInSeconds * fps)}
        durationInFrames={Math.round((segment.sourceOutSeconds - segment.sourceInSeconds) * fps)}
        playbackRate={segment.playbackRate}
        style={{position: 'absolute', width: source.width * scale, height: source.height * scale,
          maxWidth: 'none', left: -crop.x * scale, top: -crop.y * scale}} />
    </div>
  </div>;
};

const Caption = ({title}: {title: string}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const enter = spring({frame, fps, config: {damping: 24, stiffness: 160}});
  return <div style={{position: 'absolute', left: 34, top: 29, fontSize: 38,
    fontWeight: 650, letterSpacing: '-1.2px', transform: `translateY(${(1 - enter) * 5}px)`,
    opacity: interpolate(frame, [0, 7], [0.6, 1], clamp)}}>{title}</div>;
};

const Close = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const settle = spring({frame, fps, config: {damping: 26, stiffness: 100}});
  return <AbsoluteFill style={{alignItems: 'center', justifyContent: 'center', gap: 25}}>
    <div style={{display: 'flex', alignItems: 'center', gap: 17,
      transform: `scale(${interpolate(settle, [0, 1], [0.97, 1])})`}}>
      <Img src={staticFile('brand/secblitz.svg')} style={{width: 71, height: 80}} />
      <span style={{fontSize: 49, fontWeight: 700, letterSpacing: '-2px'}}>SecBlitz</span>
    </div>
    <div style={{fontSize: 43, fontWeight: 550, letterSpacing: '-1.5px'}}>A safer PC. Without headaches.</div>
    <div style={{height: 4, width: 64, borderRadius: 4, background: color.green}} />
  </AbsoluteFill>;
};

export const Demo = ({manifest}: {manifest: Manifest}) => {
  const env = useRemotionEnvironment();
  const end = footageFrames(manifest);
  if (manifest.status !== 'ready' || !manifest.segments.length) {
    if (env.isRendering) throw new Error('Actual capture and verified sources.json are required.');
    return <AbsoluteFill style={{background: color.paper, color: color.ink, justifyContent: 'center',
      alignItems: 'center', fontFamily: 'Schibsted', fontSize: 30}}>Awaiting actual Windows VM capture</AbsoluteFill>;
  }
  // A caption persists across speed changes within one phase.
  const phases = manifest.segments.filter((s, i, all) => i === 0 || s.phase !== all[i - 1].phase);
  return <AbsoluteFill style={{background: color.paper, color: color.ink, fontFamily: 'Schibsted'}}>
    {manifest.segments.map(segment => <Sequence key={segment.id} name={segment.id}
      from={segment.outputFromFrame} durationInFrames={segment.outputDurationFrames}>
      <Capture segment={segment} source={manifest.sources.find(s => s.id === segment.sourceId)!} />
    </Sequence>)}
    {phases.map((segment, i) => <Sequence key={segment.id} name={titles[segment.phase]}
      from={segment.outputFromFrame}
      durationInFrames={(phases[i + 1]?.outputFromFrame ?? end) - segment.outputFromFrame}>
      <Caption title={titles[segment.phase]} />
    </Sequence>)}
    <Sequence durationInFrames={end}>
      <div style={{position: 'absolute', right: 35, top: 36, display: 'flex', alignItems: 'center', gap: 9}}>
        <Img src={staticFile('brand/secblitz.svg')} style={{width: 23, height: 26}} />
        <span style={{fontSize: 19, fontWeight: 650}}>SecBlitz</span>
      </div>
    </Sequence>
    {manifest.closeFrames > 0 && (
      <Sequence from={end} durationInFrames={manifest.closeFrames}><Close /></Sequence>
    )}
  </AbsoluteFill>;
};
