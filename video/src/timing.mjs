export const segmentFrames = (segment, fps) =>
  Math.round((segment.sourceOutSeconds - segment.sourceInSeconds) * fps / segment.playbackRate);

export const footageFrames = (manifest) => manifest.segments.reduce(
  (end, segment) => Math.max(end, segment.outputFromFrame + segment.outputDurationFrames), 0,
);

export const totalFrames = (manifest) => footageFrames(manifest) + manifest.closeFrames;
