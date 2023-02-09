use std::process::{Command, Stdio};

use anyhow::{ensure, Context};
use tracing::info;

use crate::{
    config::Opts,
    vertspec::{ProgdefDesc, VertSpec},
};

/// build docker image and upload to the appropriate registry
pub async fn build_docker_image(vs: VertSpec, opt: &Opts) -> anyhow::Result<ProgdefDesc> {
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
        .context("docker did not output non-utf8")?
        .trim()
        .to_string();

    let ret = vs.docker_tag(&image_content_id)?;
    let tag = ret.image_name();

    let res = Command::new("docker")
        .arg("tag")
        .arg(&image_content_id)
        .arg(&tag)
        .status()?;
    ensure!(res.success(), "docker tag failed");

    if opt.local {
        let out = Command::new("minikube")
            .arg("image")
            .arg("ls")
            .stdout(Stdio::piped())
            .output()?;
        let current_images = String::from_utf8(out.stdout)?;
        if current_images.contains(&tag) {
            info!("image {} already loaded", tag);
        } else {
            info!("loading image: {}", tag);
            Command::new("minikube")
                .arg("image")
                .arg("load")
                .arg(&tag)
                .output()?;
        }
    } else {
        unimplemented!("this program can't yet upload to remote registry");
    }

    Ok(ret)
}
