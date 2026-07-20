# sim-lib-stream-wrist

In one line: Watch and wearable sensor streams that are strict enough for tests before a real wrist device exists.

## What it gives you

A wearable can report heart rate, motion, GPS, battery, connection state, touch, buttons, and other wrist-side signals. This crate puts those signals into one checked stream event shape with a sensor tag, sequence number, confidence score, and payload. Modeled sources produce repeatable samples from an index, so demos and CI can exercise wrist behavior without a watch, driver, clock, random seed, or network.

## Why you will be glad

- Every worn sample rejects malformed tags and fields, so bad device input is caught at the boundary.
- The modeled sources give stable heart-rate, motion, location, battery, and connection streams for examples and tests.
- Microphone input is raw framed audio only, keeping transcripts and voice commands out of the sensor contract.

## Where it fits

This crate extends the stream-device base for concrete worn devices. It sits above stream-core packets and below hardware adapters, watch bridges, and product code that wants wrist-side state as ordinary SIM stream data.
