//! Thin repository-tool launcher: defers to the shared sim-tooling commands.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(args: Vec<String>) -> Result<(), String> {
    let program = args.first().map(String::as_str).unwrap_or("xtask");
    let command = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| usage(program))?;
    if !matches!(command, "simdoc" | "check-file-sizes") {
        return Err(usage(program));
    }

    let root = env::current_dir().map_err(|err| format!("current dir: {err}"))?;
    let manifest = locate_sim_tooling_manifest(&root)?;
    let mut command = Command::new("cargo");
    command.args(["run", "--manifest-path"]);
    command.arg(manifest);
    command.args(["--quiet", "--"]);
    command.arg(args[1].as_str());
    command.arg("--repo-root");
    command.arg(&root);
    for arg in args.iter().skip(2) {
        command.arg(arg);
    }

    let status = command
        .status()
        .map_err(|err| format!("run shared sim-tooling command: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "shared sim-tooling command failed with status {status}"
        ))
    }
}

fn usage(program: &str) -> String {
    format!(
        "usage: {program} simdoc [--check] [--rustdoc auto|skip|force] | {program} check-file-sizes"
    )
}

fn locate_sim_tooling_manifest(repo_root: &Path) -> Result<PathBuf, String> {
    if let Ok(path) = env::var("SIMDOC_TOOLING_MANIFEST") {
        return Ok(PathBuf::from(path));
    }
    let sibling = repo_root
        .parent()
        .unwrap_or(repo_root)
        .join("sim-tooling")
        .join("Cargo.toml");
    if sibling.is_file() {
        return Ok(sibling);
    }
    Err("set SIMDOC_TOOLING_MANIFEST to the sim-tooling Cargo.toml".to_owned())
}
