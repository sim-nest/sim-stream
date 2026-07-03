# sim-lib-stream-clock

In one line: The part that lines up a stream of data with real time or musical beats.

## What it gives you

Streams arrive as a bare sequence of packets, and this crate answers the question of when each one happens. A clock chart maps positions in a stream to moments on a timeline: video frames counted per second, or MIDI ticks paced by a tempo map that can change speed partway through. You give it an instant and it returns the matching index; you give it an index and it returns the instant back, reporting whether the conversion was exact or rounded. All of this math stays in library space, so the runtime keeps carrying plain tick values while the richer clock meaning lives here, ready to align, seek, and compare streams by time or by beat.

## Why you will be glad

- The same stream can be read by wall-clock seconds or by musical beats, whichever suits the task.
- Conversions tell you when a result was rounded, so timing surprises never hide.
- Tempo can shift across a piece and the clock still tracks each moment correctly.

## Where it fits

Time alignment is its own concern in SIM, kept out of the core stream shape and out of the kernel. When audio, MIDI, or event streams need to agree on when things occur -- to sync, to seek, or to replay in order -- this crate supplies the charts and conversions. It leans on the core packet surface and adds only the timing layer above it.
