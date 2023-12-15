use std::{collections::HashMap, process::Command};

use anyhow::{ensure, Context};
use sha2::{Digest, Sha256};

use crate::{common::Executor, vertspec::VertSpec};

/// build docker image and upload to the appropriate registry
pub fn build_docker_image(vs: VertSpec) -> anyhow::Result<Executor> {
    let build_opt_flags: Vec<String> = vs
        .docker_build_args
        .iter()
        .map(|(k, v)| format!("--build-arg={}={}", k, v))
        .collect();

    let out = Command::new("docker")
        .arg("build")
        .args(build_opt_flags)
        .arg("--quiet")
        .arg("--file")
        .arg(vs.dockerfile.to_str().unwrap())
        .arg(vs.docker_build_context.to_str().unwrap())
        .output()?;
    ensure!(
        out.status.success(),
        "docker build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let image_content_id = String::from_utf8(out.stdout)
        .context("docker did output non-utf8")?
        .trim()
        .to_string();

    let name = format!("vert_{}", shorthex(hash(&vs, &image_content_id)));

    let res = Command::new("docker")
        .arg("tag")
        .arg(&image_content_id)
        .arg(&name)
        .status()?;
    ensure!(res.success(), "docker tag failed");

    Ok(Executor {
        image: name,
        inputs: vs.inputs,
        outputs: vs.outputs,
    })
}

fn hash(vertspec: &VertSpec, image_hash: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(stable_map_hash(&vertspec.docker_build_args));
    hasher.update(stable_map_hash(&vertspec.inputs));
    hasher.update(stable_map_hash(&vertspec.outputs));
    hasher.update(image_hash.as_bytes());
    hasher.finalize().into()
}

/// hash where if map a == map b then hash a == hash b, this ensures that keys are sorted
fn stable_map_hash(map: &HashMap<String, String>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let mut keys: Vec<(&str, &str)> = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    keys.sort();
    let js = serde_json::to_vec(&keys).unwrap();
    hasher.update(js);
    hasher.finalize().into()
}

pub fn shorthex(bytes: [u8; 32]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .take(16)
        .collect()
}
