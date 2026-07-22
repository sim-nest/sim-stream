# sim-lib-stream-device

In one line: A hardware-free device sample lane, so sensors can be modeled and tested before any real device is attached.

## What it gives you

Device work often stalls on access to physical gear. This crate gives each device stream a small shared sample shape with a stable kind tag, a sequence number, and strict round-trip rules. A modeled source produces those samples from a counter, so a glasses, watch, badge, or lab rig can ship tests that never depend on a clock, network, driver, or sensor.

## Why you will be glad

- Sensor samples fail closed when their shape is wrong, so bad hardware input cannot quietly become plausible data.
- Modeled sources make device behavior reproducible in CI, demos, and notebooks.
- Every sample travels as ordinary stream data, so stream tools can inspect it without a separate device protocol.

## Where it fits

This is the shared base for SIM device streams. It sits above stream-core packets and below concrete providers, adapters, and hardware crates. Product-specific code extends the sample kinds and modeled sources while keeping the same deterministic sequence contract.
