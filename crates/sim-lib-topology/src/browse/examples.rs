use super::TopologyExampleSpec;

/// Returns the generated `.simtopo` package examples used by browse Cards.
pub fn topology_example_specs() -> Vec<TopologyExampleSpec> {
    vec![
        example(
            "pipeline",
            "linear in -> work -> out pipeline",
            PIPELINE_EXAMPLE,
        ),
        example(
            "branch",
            "bool branch with true and false routes",
            BRANCH_EXAMPLE,
        ),
        example("loop", "bounded branch loop with max-visits", LOOP_EXAMPLE),
        example("star", "tee fanout into merge join", STAR_EXAMPLE),
        example(
            "stream-patch",
            "stream-shaped patch using ordinary topology nodes",
            STREAM_PATCH_EXAMPLE,
        ),
        example(
            "table-sync",
            "cell-backed table synchronization sketch",
            TABLE_SYNC_EXAMPLE,
        ),
        example(
            "self-patching",
            "patch-producing topology shape",
            SELF_PATCHING_EXAMPLE,
        ),
        example(
            "daw-session-launch",
            "DAW session launch adapter topology",
            DAW_SESSION_EXAMPLE,
        ),
    ]
}

fn example(
    name: &'static str,
    summary: &'static str,
    package: &'static str,
) -> TopologyExampleSpec {
    TopologyExampleSpec {
        name,
        summary,
        package,
    }
}

const PIPELINE_EXAMPLE: &str = r#"graph:
topology example-pipeline
node in verb=in
node work verb=wire
node out verb=out
wire in -> work
wire work -> out

tests:
smoke input="seed" expect="seed"
"#;

const BRANCH_EXAMPLE: &str = r#"graph:
topology example-branch
node in verb=in
node gate verb=branch
node out verb=out
wire in -> gate
wire gate:true -> out
wire gate:false -> out

tests:
smoke input=true expect=true
"#;

const LOOP_EXAMPLE: &str = r#"graph:
topology example-loop
node in verb=in
node gate verb=branch
node out verb=out
wire in -> gate
wire gate:false -> gate max_visits=2
wire gate:true -> out
budget max-steps=12

tests:
smoke input=true expect=true
"#;

const STAR_EXAMPLE: &str = r#"graph:
topology example-star
node in verb=in
node tee verb=tee
node join verb=merge in=[left,right]
node out verb=out
wire in -> tee
wire tee -> join:left
wire tee -> join:right
wire join -> out

tests:
smoke input="seed" expect=["seed","seed"]
"#;

const STREAM_PATCH_EXAMPLE: &str = r#"graph:
topology example-stream-patch
node in verb=in
node monitor verb=tee in=[stream]
node tap verb=wire
node out verb=out
wire in -> monitor:stream
wire monitor -> tap
wire tap -> out
meta media="stream"

tests:
smoke input="packet" expect="packet"
"#;

const TABLE_SYNC_EXAMPLE: &str = r#"graph:
topology example-table-sync
node in verb=in
node write verb=cell name=table op=write emit=input
node out verb=out
cell table initial=nil
wire in -> write
wire write -> out
meta storage="table-sync"

tests:
smoke input="row" expect="row"
"#;

const SELF_PATCHING_EXAMPLE: &str = r#"graph:
topology example-self-patching
node in verb=in
node proposal verb=patch mode=produce
node out verb=out
wire in -> proposal
wire proposal -> out
meta pattern="self-patching"
"#;

const DAW_SESSION_EXAMPLE: &str = r#"graph:
topology example-daw-session-launch
node in verb=in
node session verb=call target=daw/session role=daw/load
node render verb=call target=daw/render-offline role=daw/render
node out verb=out
wire in -> session
wire session -> render
wire render -> out
meta adapter="daw-session"
"#;
