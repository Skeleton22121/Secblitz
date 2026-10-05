export type Source = {
  id: string;
  file: string;
  sha256: string;
  durationSeconds: number;
  width: number;
  height: number;
};

export type Segment = {
  id: string;
  sourceId: string;
  phase: 'check' | 'choose' | 'fix' | 'undo' | 'tools';
  sourceInSeconds: number;
  sourceOutSeconds: number;
  playbackRate: number;
  outputFromFrame: number;
  outputDurationFrames: number;
  evidence: string;
  crop?: {x: number; y: number; width: number; height: number};
};

export type Manifest = {
  status: string;
  fps: number;
  width: number;
  height: number;
  closeFrames: number;
  posterFrame: number | null;
  fixturesUsed: boolean | null;
  provenance: Record<string, string | null>;
  sources: Source[];
  segments: Segment[];
};
