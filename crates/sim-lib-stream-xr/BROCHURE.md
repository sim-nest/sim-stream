# sim-lib-stream-xr

In one line: Repeatable glasses sensor streams for XR work without needing a Viture or Halo device on the test bench.

## What it gives you

XR glasses need more than a screen feed. A rich pair reports pose, cameras, hands, taps, and microphone chunks; a lighter pair reports motion hints, taps, camera references, and audio chunks. This crate puts those inputs into strict stream samples with stable tags and sequence numbers. Modeled sources produce the same Viture-style and Halo-style samples every run, so adapters, demos, and CI can exercise glasses behavior without a driver, clock, random seed, network, or physical hardware.

## Why you will be glad

- Bad tags, missing fields, and malformed numbers are rejected at the stream boundary.
- Viture-style pose, stereo camera, and hand samples stay separate from Halo-style motion, tap, camera, and mic chunk samples.
- Microphone input is only a reference to captured audio, so transcripts and intent labels stay outside the sensor contract.

## Where it fits

This crate extends the stream-device base for glasses devices. It sits above stream-core packet shapes and below XR hardware providers, reprojection adapters, HUD renderers, and product code that consumes glasses-side inputs as ordinary SIM stream data.
