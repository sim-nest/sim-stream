# sim-stream

sim-stream gives you a checked, packet-by-packet model of changing data --
audio, MIDI, clocks, and any timed stream -- that you compose and run inside a
SIM runtime.

SIM is a small Rust protocol kernel plus loadable libraries; the `sim` CLI
installs with `cargo install sim-run`, and sim-say is the full walkthrough.
sim-stream is a library set, so you reach for it from Rust.

## Example

Describe a validated PCM audio format -- the immutable contract every buffer,
source, and sink in the substrate shares:

```bash
cargo add sim-lib-stream-audio
```

```rust
use sim_lib_stream_audio::{PcmSampleFormat, PcmSpec};

// Rejects a zero channel count or zero sample rate, so a value that
// constructs is always a usable audio configuration.
let spec = PcmSpec::f32(2, 48_000)?;
assert_eq!(spec.channels(), 2);
assert_eq!(spec.sample_rate_hz(), 48_000);
assert_eq!(spec.sample_format(), PcmSampleFormat::F32);
# Ok::<(), sim_kernel::Error>(())
```

(from the passing doctest in
`crates/sim-lib-stream-audio/src/spec.rs:14`)

## How it works

This repo holds two library families that sit on top of `sim-kernel` -- a
streaming substrate (in-memory stream values, clocks, combinators, and audio
adapters) and the rank library for coordinate, order, and search spaces.

The streaming libraries keep media I/O, clock math, and stream composition in
library space: kernel events carry only `Tick` values and datum refs, while the
concrete envelopes, packets, buffers, and spines live here. The rank library
provides the coordinate and ordering surface those and other runtime objects
draw on. Every library installs into a runtime as a capability-gated
`sim_kernel::Lib`.

## Crates

### Streaming substrate

- `sim-lib-stream-core` -- the core stream value surface: envelopes, metadata,
  packets, buffers, and lazy spines, plus the shapes and read/construct classes
  that describe them. Packet observation uses kernel event helpers and refs;
  no server frame kind or media hook is hardwired into the kernel.
- `sim-lib-stream-clock` -- clock charts and tick conversion helpers. Keeps
  clock math in library space while kernel events still carry only `Tick`
  values, with clock indexes interned as datum content refs.
- `sim-lib-stream-combinators` -- lazy, in-memory stream combinators that
  compose stream-core packet spines without touching devices, files, or
  transports.
- `sim-lib-stream-audio` -- PCM source and sink adapters. The in-memory source
  and sink are deterministic backends; stream observation still flows through
  stream-core packets and spines.
- `sim-lib-stream-prelude` -- the umbrella library that composes core, audio,
  and combinators into a single host-registered library. It installs
  capability-gated functions for opening deterministic memory MIDI and PCM
  sources and sinks, running source-to-sink pipelines, applying combinator
  stages, and browsing stream handles as Cards; `StreamHandle` is the live
  handle threaded through every stream operation.

### Rank

- `sim-lib-rank` -- coordinate, codec, order, search, and optional domain
  spaces. Provides rank coordinates and grades, an ordering and scoring surface,
  search and retrieval over rank spaces, rank codecs, and optional domain
  features behind feature flags.

## Stream values

The stream substrate is layered. `sim-lib-stream-core` defines the in-memory
value surface -- `StreamEnvelope` carries metadata, capabilities, and a
transport profile; `StreamPacket` variants (`DataPacket`, `MidiPacket`,
`PcmPacket`) carry observed media; buffers apply a `BufferPolicy` with an
overflow policy and backpressure outcome; and spines hold the lazy sequence of
packets. The clock, combinator, and audio libraries build only on these
values, and the prelude composes them into one installable library so that
sources, sinks, pipelines, and combinator stages share a single packet model.

## Validation

These commands run in the constellation workspace; only `sim-kernel` builds from a lone clone today (see `DEVELOPING.md` in `sim-sdk`). A single-repo build lands with the first crates.io publish.

```bash
cargo fmt --check && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo doc --workspace --no-deps
cargo run -p xtask -- simdoc --check
```

## Documentation Lanes

`cargo run -p xtask -- simdoc` builds the public documentation lanes:

- API docs: `target/doc/`
- Agent cards: `docs/agents/cards.jsonl` and `docs/agents/card-index.json`
- Human docs: `docs/humans/`
- Diagrams: `docs/diagrams/src/` and `docs/diagrams/generated/`

The same command writes split contract files under `docs/generated/`.
