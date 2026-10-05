import {
  AbsoluteFill, Easing, OffthreadVideo, Sequence, interpolate,
  staticFile, useCurrentFrame, useRemotionEnvironment, useVideoConfig,
} from 'remotion';
import type {Camera, LoopManifest, LoopSegment, Source} from './manifest';

// Silent website loop: real footage only, no text, no outro. Each cut eases a
// virtual camera from `from` to `to`; the last frame matches the first.
const ease = Easing.bezier(0.45, 0, 0.2, 1);

/** Keep the view inside the recording so no edge ever shows. */
export const clampCamera = (c: Camera, source: Source, aspect: number): Camera => {
  const zoom = Math.max(1, c.zoom);
  const w = source.width / zoom;
  const h = Math.min(w / aspect, source.height);
  return {
    zoom,
    x: Math.min(Math.max(c.x, w / 2), source.width - w / 2),
    y: Math.min(Math.max(c.y, h / 2), source.height - h / 2),
  };
};

const Shot = ({segment, source}: {segment: LoopSegment; source: Source}) => {
  const frame = useCurrentFrame();
  const {fps, width, height} = useVideoConfig();
  const t = interpolate(frame, [0, Math.max(1, segment.outputDurationFrames - 1)], [0, 1],
    {extrapolateLeft: 'clamp', extrapolateRight: 'clamp', easing: ease});
  const {from, to} = segment.camera;
  const cam = clampCamera({
    x: interpolate(t, [0, 1], [from.x, to.x]),
    y: interpolate(t, [0, 1], [from.y, to.y]),
    // Zoom moves in log space so it feels even at every scale.
    zoom: Math.exp(interpolate(t, [0, 1], [Math.log(from.zoom), Math.log(to.zoom)])),
  }, source, width / height);
  const scale = (width / source.width) * cam.zoom;
  return <AbsoluteFill style={{overflow: 'hidden'}}>
    <OffthreadVideo muted src={staticFile(source.file)}
      trimBefore={Math.round(segment.sourceInSeconds * fps)}
      playbackRate={segment.playbackRate}
      style={{position: 'absolute', width: source.width, height: source.height, maxWidth: 'none',
        transformOrigin: '0 0',
        transform: `translate(${width / 2 - cam.x * scale}px, ${height / 2 - cam.y * scale}px) scale(${scale})`}} />
  </AbsoluteFill>;
};

export const Loop = ({manifest}: {manifest: LoopManifest}) => {
  const env = useRemotionEnvironment();
  if (manifest.status !== 'ready' || !manifest.segments.length) {
    if (env.isRendering) throw new Error('Actual capture and verified sources-v070.json are required.');
    return <AbsoluteFill style={{background: '#f7f7f8'}} />;
  }
  return <AbsoluteFill style={{background: '#f7f7f8'}}>
    {manifest.segments.map(segment => <Sequence key={segment.id} name={segment.id}
      from={segment.outputFromFrame} durationInFrames={segment.outputDurationFrames}>
      <Shot segment={segment} source={manifest.sources.find(s => s.id === segment.sourceId)!} />
    </Sequence>)}
  </AbsoluteFill>;
};
