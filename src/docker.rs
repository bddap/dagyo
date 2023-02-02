use std::{collections::HashMap, path::Path};

use bollard::Docker;
use futures::TryStreamExt;
use sha2::{Digest, Sha256};

use crate::vertspec::VertSpec;

pub type ImageName = String;

pub async fn build_docker_image(vs: &VertSpec) -> anyhow::Result<ImageName> {
    let docker = Docker::connect_with_local_defaults()?;
    let tar = tarchive(&vs.docker_build_context, &vs.dockerfile)?;
    let tarh = hash(&tar, &vs.docker_build_args);

    let buildargs = vs
        .docker_build_args
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let options = bollard::image::BuildImageOptions {
        dockerfile: "Dockerfile",
        buildargs,
        t: &tarh.clone(),
        ..Default::default()
    };
    let mut stream = docker.build_image(options, None, Some(tar.into()));
    while let Some(item) = stream.try_next().await? {
        if let Some(stream) = item.stream {
            eprint!("{}", stream);
        }
    }
    Ok(tarh)
}

/// currently does not heed .dockerignore
fn tarchive(docker_build_context: &Path, dockerfile: &Path) -> anyhow::Result<Vec<u8>> {
    if docker_build_context.join("Dockerfile").exists() {
        anyhow::ensure!(
            docker_build_context.join("Dockerfile") == dockerfile,
            "Docker build context may only contain 'Dockerfile' if vertpec points to it. \
             This is a limitation in the builder."
        );
    }

    let mut archive = tar::Builder::new(Vec::new());
    archive.append_dir_all(".", docker_build_context)?;
    archive.append_path_with_name(dockerfile, "Dockerfile")?;
    let archive = archive.into_inner()?;
    Ok(archive)
}

fn hash(preimage: &[u8], build_args: &HashMap<String, String>) -> String {
    let mut sorted = build_args.iter().collect::<Vec<_>>();
    sorted.sort();
    let build_args_serialized = serde_json::to_vec(&sorted).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(&preimage);
    hasher.update(&build_args_serialized);
    let hash = hasher.finalize();

    // docker specifcally can't handle 64-byte hexidecimal strings, so we prepend
    // some characters
    format!("dagyo_executor_{:x}", hash)
}
