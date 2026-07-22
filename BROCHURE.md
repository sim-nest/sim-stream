# sim-stream

In one line: the shape of changing data -- anything that moves, watched packet by packet and wired into a checked plan.

## What it gives you

One shared form for a run of changing data, clocks that line a stream up with real time or musical beats, building blocks for reshaping and combining flows, audio streams, an exact position for every item in a well-defined space, and a topology layer that turns a described network of connected steps into a checked, ready-to-run plan.

## Why you will be glad

Streams, audio, ranks, and topologies all speak one packet-by-packet language, so a moving signal can be observed, reshaped, tested, and replayed without inventing a new surface for each medium. Timing stays explicit, routing is checked before work runs, and deterministic in-memory fixtures make live-looking behavior practical to test on any machine.

## Where it fits

sim-stream is the changing-data layer above the SIM kernel. It keeps stream packets, clocks, rank spaces, combinators, audio adapters, and topology plans in loadable libraries, so runtime hosts can add streaming behavior without baking media, graph, or ordering policy into the kernel.
