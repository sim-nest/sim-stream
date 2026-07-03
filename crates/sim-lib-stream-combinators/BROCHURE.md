# sim-lib-stream-combinators

In one line: The building blocks for reshaping and combining streams of data as they flow.

## What it gives you

Once data is moving as a stream, you rarely want it untouched. This crate supplies the stages that transform a stream lazily, only doing work as packets are pulled: reshape each item, drop the ones that do not match, group them into windows, or tap them to watch diagnostics go by. It can join two streams on a shared field, rank items by a numeric field, and project or redact parts of each record so sensitive fields never leak downstream. For live data it adds practical helpers -- resampling audio, a jitter buffer, a latency delay, and a rate gate for bursty events. A recording captures a finished stream so it can be replayed, seeked, and read again exactly.

## Why you will be glad

- Streams are shaped without copying everything up front, since each stage runs only as packets are pulled.
- Sensitive fields can be projected or redacted mid-flow, so downstream stages never see what they should not.
- A captured run replays exactly, so a live moment can be studied again and again offline.

## Where it fits

This is where streams become useful rather than merely present. It composes the core packet spines without touching devices, files, or transports, so the same stages apply to audio, events, diagnostics, and model output alike. Higher surfaces can lower graph forms into these stages, making it the shared toolkit for filtering, joining, ranking, and replaying any stream in SIM.
