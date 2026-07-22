//! Deterministic cookbook builders for stream-audio recipes.

use sim_kernel::{Expr, NumberLiteral, Symbol};

use crate::{MemoryPcmSink, PcmBuffer, PcmSink, PcmSpec};

/// Build the modeled PCM spec descriptor used by the cookbook recipe.
pub fn pcm_spec_demo() -> Expr {
    let spec = PcmSpec::f32(2, 48_000).expect("valid cookbook PCM spec");
    let mut sink = MemoryPcmSink::new(spec);
    sink.write_buffer(
        PcmBuffer::f32(spec, 2, vec![0.25, -0.25, 0.5, -0.5]).expect("valid cookbook PCM buffer"),
    )
    .expect("memory sink accepts matching PCM spec");
    sink.flush().expect("memory sink flushes");

    Expr::Map(vec![
        (field("kind"), sym("stream-audio", "pcm-spec")),
        (field("format"), sym("stream-audio/pcm", "f32-interleaved")),
        (field("channels"), number(spec.channels())),
        (field("sample-rate"), number(spec.sample_rate_hz())),
        (field("frames"), number(2)),
        (field("sink-buffers"), number(sink.buffers().len())),
        (field("flush-count"), number(sink.flush_count())),
    ])
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified("stream-audio", name))
}

fn sym(namespace: &str, name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified(namespace, name))
}

fn number(value: impl ToString) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::qualified("numbers", "i64"),
        canonical: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_spec_demo_uses_matching_memory_sink() {
        let Expr::Map(entries) = pcm_spec_demo() else {
            panic!("PCM spec demo is a map")
        };
        assert!(entries.iter().any(|(_, value)| {
            matches!(value, Expr::Symbol(symbol) if symbol.as_qualified_str() == "stream-audio/pcm/f32-interleaved")
        }));
    }
}
