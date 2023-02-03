use std::path::Path;

use bollard::Docker;
use futures::TryStreamExt;

use crate::vertspec::{ProgdefHash, VertSpec};

pub async fn build_docker_image(vs: &VertSpec) -> anyhow::Result<ProgdefHash> {
    let docker = Docker::connect_with_local_defaults()?;
    let tar = tarchive(&vs.docker_build_context, &vs.dockerfile)?;
    let progdef_hash = vs.content_hash(&tar);

    let buildargs = vs
        .docker_build_args
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let options = bollard::image::BuildImageOptions {
        dockerfile: "Dockerfile",
        buildargs,
        t: &progdef_hash.image_name(),
        ..Default::default()
    };
    let mut stream = docker.build_image(options, None, Some(tar.into()));
    while let Some(item) = stream.try_next().await? {
        if let Some(stream) = item.stream {
            eprint!("{}", stream);
        }
    }
    Ok(progdef_hash)
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
