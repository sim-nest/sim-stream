#![forbid(unsafe_code)]
//! Repository maintenance tasks for sim-stream.

mod simdoc;

fn main() {
    if let Err(err) = simdoc::run(std::env::args().collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
