# Provider-neutral rank fusion

Combines semantic, lexical, and freshness rankings with weighted reciprocal-rank fusion. The three
source names are illustrative: the primitive accepts any stable keys and source ids, performs no
network work, and retains a receipt from which every output score and ordering decision is derived.
