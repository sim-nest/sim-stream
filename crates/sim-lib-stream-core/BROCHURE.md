# sim-lib-stream-core

In one line: The shared shape for a run of changing data, so anything that moves can be watched packet by packet.

## What it gives you

A stream in SIM is data that arrives over time -- audio samples, events, diagnostics, model output -- and this crate defines the common container for it. Every stream carries a small header describing what it is and which direction it flows, then delivers its contents as a sequence of packets. Each packet is a self-describing unit you can observe, count, buffer, and read back without guessing its layout. Buffers hold packets under a stated overflow policy, an inspector reports on what passed through, and a cassette captures a whole run so a finished stream can be stored and revisited. It is deliberately small: one honest value surface every other stream part builds on.

## Why you will be glad

- Any moving data -- sound, events, or reports -- speaks the same packet shape, so tools written once work everywhere.
- Every packet describes itself, so you can watch a run without decoding hidden formats.
- A run can be captured to a cassette and read back exactly, so nothing observed is lost.

## Where it fits

This is the foundation of SIM's streaming fabric. It holds no devices, files, or network code -- only the in-memory shapes that clocks, audio adapters, and combinators layer on top. When SIM needs to move changing data through the runtime and let it be observed as ledgered events, everything starts from the envelopes, packets, and buffers defined here.
