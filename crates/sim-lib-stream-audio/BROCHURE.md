# sim-lib-stream-audio

In one line: The part that turns raw sound samples into observable streams and back again.

## What it gives you

Sound in a computer is a long list of numbers, and this crate moves those numbers in and out of SIM's stream shape. It reads a PCM source into a stream of packets and writes a stream back out to a PCM sink, so recorded or generated audio can be watched and handled like any other moving data. Along the way it converts between common sample formats -- whole-number and floating-point -- and between interleaved and channel-separated layouts, checking that every conversion stays in range. The built-in source and sink work fully in memory and produce the same result every run, which makes audio behavior easy to test without a sound card.

## Why you will be glad

- Audio flows through the exact same packet surface as events and diagnostics, so one set of tools handles it all.
- Format and channel conversions are checked, so out-of-range samples are caught instead of silently wrapping.
- In-memory backends give identical output every run, so sound handling can be tested anywhere.

## Where it fits

This is SIM's bridge between raw sound and the streaming fabric. It holds no driver or hardware code; the in-memory source and sink stand in for real devices while keeping results deterministic. Observation still travels through the core packets and spines, so audio joins events and reports as one kind of stream the rest of the system already understands.
