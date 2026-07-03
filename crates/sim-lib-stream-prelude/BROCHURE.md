# sim-lib-stream-prelude

In one line: The ready-made bundle that makes SIM's whole streaming toolkit available in one install.

## What it gives you

This is the single package that gathers the streaming parts -- core packets, audio adapters, and combinators -- into one library the runtime can load at once. Installing it registers a set of named, permission-gated operations: open a deterministic in-memory MIDI or PCM source or sink, run a source-to-sink pipeline, apply combinator stages to reshape the flow, and browse any live stream as a card you can read. Each stream you open is tracked through a single handle that threads every operation, so opening, pumping, transforming, and inspecting all speak of the same thing. Because access is capability-gated, a host decides exactly which stream powers a given run may use.

## Why you will be glad

- One install brings the entire streaming fabric online, so callers need not wire the parts together by hand.
- Every stream operation is permission-gated, so a host grants only the access a given run should have.
- Live streams show up as readable cards, so you can inspect what is flowing without extra tooling.

## Where it fits

This is the front door to streaming for SIM's Lisp-facing callers. It composes the lower crates and its own prerequisites into one registered library, names the functions and capabilities they expose, and hands back the handle that every stream operation shares. When a program wants to work with sound, events, or reports as streams, this is the layer it actually calls.
