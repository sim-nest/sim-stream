# sim-lib-rank

In one line: A way to give every item in a well-defined space an exact number, so collections can be ordered and searched.

## What it gives you

Rank turns a structured space of possibilities into a line of numbered positions. Given a grammar that describes what a valid item looks like -- a coordinate, a melody, a duration, a whole musical phrase -- it assigns each item a unique ordinal and can turn that ordinal back into the item exactly. This two-way mapping makes any conforming space indexable: you can order candidates, score them, and walk toward the best one by hill-climbing or beam search across neighbors. It ships bounded example spaces, including finite music spaces of pitches, rhythms, and progressions, plus codecs, ordering, and search over them. The mapping is deterministic, so the same space always numbers the same way.

## Why you will be glad

- Anything describable by a grammar becomes searchable by number, so retrieval and ordering share one mechanism.
- The mapping runs both ways exactly, so a position and its item are always recoverable from each other.
- Search walks toward higher-scoring items step by step, so good candidates are found without scanning everything.

## Where it fits

Rank is SIM's coordinate-and-order engine, used wherever changing data must be sorted, scored, or retrieved by position -- including ranking stream items by a field. It stays in library space with its own codecs, spaces, and search, offering a shared way to number and order structured values rather than hardwiring any single ordering into the runtime.
