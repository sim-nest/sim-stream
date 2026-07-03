# sim-lib-topology

In one line: The part that turns a described network of connected steps into a checked, ready-to-run plan.

## What it gives you

A topology is a wiring diagram: boxes that do something, joined by lines that say what feeds what. This crate lets you author that wiring as plain graph data, short text, a diagram, or a packaged file, then checks it and compiles it into a deterministic runtime plan. It confirms that nodes connect sensibly, carries along the capabilities a topology needs, and can hold small tests that state an input and the output expected from it. The compiled plan lists its nodes in a settled order, so the same description always produces the same shape. You can also present a topology as a readable card, making the structure something a person can browse rather than infer.

## Why you will be glad

- The same flow can be written as data, text, or a diagram, so authors pick the form that reads clearest.
- A topology is validated before it runs, so broken wiring is caught early instead of mid-run.
- Built-in tests state an input and its expected output, so a flow can prove it still behaves.

## Where it fits

Topology is how SIM describes and assembles multi-step flows without burying the wiring in code. It compiles authored graphs into deterministic plans the runtime can execute, keeping the structure of a pipeline as inspectable data. When streams, ranks, and other pieces need to be connected into a working whole, this crate turns the description of that whole into something checked and repeatable.
