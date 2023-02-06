use std::{process::Command, str::from_utf8};

use clap::Parser;
use dagyo::{
    config::Opts,
    docker::build_docker_image,
    kubestuff,
    vertspec::{Progdef, VertSpec},
};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();
    let vertspecs = VertSpec::from_file(&opts.vertspec)?;

    // build docker images for each progdef
    let mut verts = Vec::new();
    for (name_for_humans, spec) in vertspecs {
        info!("building docker image for {}", name_for_humans);
        let progdef_hash = build_docker_image(&spec).await?;
        verts.push(Progdef {
            spec,
            progdef_hash,
            name_for_humans,
        });
    }

    if opts.local {
        // This part is a little hacky for now.
        // This code takes the images se just built from our local docker daemon and make them available to minikube.
        // Need to find a faster way to do this. I takes forever.

        info!("loading images into minikube");

        let current_images = Command::new("minikube")
            .arg("image")
            .arg("ls")
            .stdout(std::process::Stdio::piped())
            .output()?
            .stdout;
        let current_images: &str = from_utf8(&current_images)?;

        for vert in &verts {
            let image_name = vert.progdef_hash.image_name();
            if current_images.contains(&image_name) {
                info!("image {} already loaded", image_name);
                continue;
            }
            info!("loading image: {}", &image_name);
            Command::new("minikube")
                .arg("image")
                .arg("load")
                .arg(image_name)
                .output()?;
        }
    }

    // and spin up a docker container for each image
    // https://levelup.gitconnected.com/two-easy-ways-to-use-local-docker-images-in-minikube-cd4dcb1a5379
    info!("spinning up containers");
    let cluster = kubestuff::Cluster::from_verts(&opts, &verts);
    cluster.apply().await?;
    info!("spun up containers");

    // manually connect source to greet and greet to some output

    // read from the output and print it

    Ok(())
}
