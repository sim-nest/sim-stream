use super::{TopologyFunctionSpec, TopologyVerbSpec};

/// Returns the metadata specs for the public topology functions.
pub fn topology_function_specs() -> Vec<TopologyFunctionSpec> {
    vec![
        spec(
            "graph",
            "build a runnable topology connection",
            "Parses graph data, validates it, compiles it, and returns a local topology connection.",
            "topology graph data",
            "server connection",
        ),
        spec(
            "compile",
            "compile topology graph data",
            "Validates graph data and returns deterministic compile-plan metadata.",
            "topology graph data",
            "compiled topology metadata",
        ),
        spec(
            "run",
            "run topology graph data once",
            "Runs graph data with one input expression through the deterministic scheduler.",
            "graph, input",
            "output expression",
        ),
        spec(
            "parse-text",
            "parse topology text",
            "Parses the line-oriented topology DSL into canonical graph data.",
            "topology text",
            "graph data",
        ),
        spec(
            "from-text",
            "parse topology text into a connection",
            "Parses topology text and returns a runnable local topology connection.",
            "topology text",
            "server connection",
        ),
        spec(
            "draw",
            "parse an ASCII topology diagram",
            "Converts an ASCII diagram into canonical graph data.",
            "diagram text",
            "graph data",
        ),
        spec(
            "from-diagram",
            "parse an ASCII diagram into a connection",
            "Converts an ASCII diagram into a runnable local topology connection.",
            "diagram text",
            "server connection",
        ),
        spec(
            "reflect",
            "reflect topology graph data",
            "Returns canonical graph data with target and cell redaction governed by topology reflection capability.",
            "graph",
            "reflected graph data",
        ),
        spec(
            "explain",
            "run and explain a topology",
            "Runs graph data with one input and returns a compact explanation of visits, edge choices, and outputs.",
            "graph, input",
            "explanation data",
        ),
        spec(
            "replay",
            "replay a reflected topology run",
            "Runs graph data with one input, records a reflected report, and replays the recorded output.",
            "graph, input",
            "output expression",
        ),
        spec(
            "counterfactual",
            "run a counterfactual topology replay",
            "Runs graph data, applies one target, edge, or predicate change, and returns the changed output.",
            "graph, input, change",
            "output expression",
        ),
        spec(
            "def",
            "register named topology graph data",
            "Stores graph data in the context topology registry.",
            "name, graph",
            "graph data",
        ),
        spec(
            "get",
            "fetch named topology graph data",
            "Returns reflected graph data with private targets and cells redacted unless topology-reflect is present.",
            "name",
            "graph data or nil",
        ),
        spec(
            "list",
            "list registered topology names",
            "Returns registered topology symbols in deterministic order.",
            "none",
            "symbol list",
        ),
        spec(
            "remove",
            "remove a named topology",
            "Removes registered graph data and returns the removed graph or nil.",
            "name",
            "graph data or nil",
        ),
        spec(
            "load-file",
            "load a topology package",
            "Compatibility wrapper that loads a .simtopo package from a host file into the topology registry.",
            "path",
            "graph data",
        ),
        spec(
            "load-source",
            "load a topology package from a table",
            "Loads a .simtopo package from an evaluated Table or Dir entry into the topology registry.",
            "table, key",
            "graph data",
        ),
        spec(
            "reload",
            "reload a topology package",
            "Reloads a registered package from its stored source descriptor.",
            "name",
            "graph data",
        ),
        spec(
            "patch",
            "apply a topology patch",
            "Applies a validated topology patch and returns a new runnable connection.",
            "graph-or-connection, patch",
            "server connection",
        ),
        spec(
            "test",
            "run embedded topology package tests",
            "Runs every embedded GraphTest in graph data, topology text, or a .simtopo package source.",
            "graph, text, or package source",
            "topology test report",
        ),
    ]
}

/// Returns the metadata specs for the core topology node verbs.
pub fn topology_verb_specs() -> Vec<TopologyVerbSpec> {
    vec![
        verb(
            "in",
            "public graph input",
            "Receives the graph input expression and emits it on out.",
        ),
        verb(
            "out",
            "public graph output",
            "Completes the graph with the received expression.",
        ),
        verb(
            "wire",
            "pass-through node",
            "Forwards input to output without changing it.",
        ),
        verb(
            "tee",
            "fanout node",
            "Emits one input on out for every outgoing edge route.",
        ),
        verb(
            "call",
            "call target node",
            "Invokes a callable value or eval-fabric target expression.",
        ),
        verb(
            "branch",
            "predicate branch node",
            "Routes bool input or predicate result through true, false, or else ports.",
        ),
        verb(
            "cell",
            "state cell node",
            "Reads, writes, appends, merges, or clears a graph-local cell.",
        ),
        verb(
            "merge",
            "join node",
            "Combines incoming values using all, any, latest, or count mode.",
        ),
        verb(
            "race",
            "first accepted value wins",
            "Emits the first accepted incoming value and ignores later arrivals.",
        ),
        verb(
            "quorum",
            "quorum vote node",
            "Emits a value after enough matching keys arrive.",
        ),
        verb(
            "reduce",
            "fold incoming values",
            "Folds incoming values with an optional callable target.",
        ),
        verb(
            "patch",
            "produce topology patch data",
            "Validates and emits topology patch data for explicit application.",
        ),
    ]
}

fn spec(
    symbol: &'static str,
    summary: &'static str,
    detail: &'static str,
    args: &'static str,
    result: &'static str,
) -> TopologyFunctionSpec {
    TopologyFunctionSpec {
        symbol,
        summary,
        detail,
        args,
        result,
    }
}

fn verb(name: &'static str, summary: &'static str, detail: &'static str) -> TopologyVerbSpec {
    TopologyVerbSpec {
        name,
        summary,
        detail,
    }
}
