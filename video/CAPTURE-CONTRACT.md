# Capture handoff

Take 03 is the sole approved source. The populated manifest records the 33-second edit.

Copy the original continuous recording into `video/public/capture/`. Keep it unmodified and calculate SHA-256. Keep timestamp notes and version evidence under `video/`. Do not include credentials or machine identifiers. Only the capture operator performs guest operations.

Populate `sources.json`, then change `status` to `ready`:

- `provenance`: original recording path, capture method, evidence of product version 0.3.1, and fixture details.
- `fixturesUsed`: explicit boolean retained for provenance. Visible diagnostic captions removed by request; provenance retained here and in sources.json.
- `sources`: `{id, file, sha256, durationSeconds, width, height}`. `file` is relative to `video/public`, for example `capture/continuous.mp4`.
- `segments`: `{id, sourceId, phase, sourceInSeconds, sourceOutSeconds, playbackRate, outputFromFrame, outputDurationFrames, evidence, crop?}`.
- `phase`: `check`, `choose`, `fix`, `undo`, or optional `tools`. Captions persist across consecutive segments of the same phase.
- Source ranges are start-inclusive and end-exclusive, in seconds from the original recording. Keep chronology and visible causal actions. Each `evidence` describes the actual action or result visible in that range.
- Meaningful actions and results run at 1x. Compressed portions use 2x through 10x. Long waits are cut.
- Output ranges are integer frames at 30 fps. Duration is `round((out - in) * 30 / rate)`. Start the first at zero and place each next immediately after the previous. The approved edit is 990 frames including a 60-frame close.
- Optional `crop` is `{x, y, width, height}` in original source pixels. It must keep relevant UI and results visible. No stretch, synthetic results or replacement terminal.
- `posterFrame`: a reviewed frame in the real footage, not the close or a blank start.

## Commands

Run inside `video/`:

1. `npm ci` and `npm run assets`
2. `npm run browser`
3. `npm run check` and `npm run verify`
4. `npm run preview` for a first real-footage frame. Additional frames: `npx remotion still src/index.ts SecblitzDemo out/review-N.png --frame=N`.
5. Review opening, selection, apply result, undo result, all cuts and close before rendering. Inspect the source recording to choose crops and timing.
6. `npm run render` produces the silent 720p30 H.264/yuv420p edit under `video/out/`.
7. Review playback and file size, targeting less than 10 MiB. Adjust CRF only if needed, while retaining readable text.
8. `npm run deliver` remuxes with faststart into `website/assets/intro-6bb434a9c067.mp4` and creates `website/assets/preview-33b342ab21fb.webp`. Existing outputs are protected against accidental overwrite. These names identify the approved delivery bytes; if bytes change, update the filename hash prefixes, delivery references, website references and staging allowlist together. Verification rejects mismatched hash prefixes. A path-only rename requires no render.

The `SecblitzDemo` composition uses OffthreadVideo and frame-based spring animation. Studio shows an explicit waiting message until handoff. Rendering that waiting state is blocked. The website HTML, CSS and JS are outside this project's ownership.
