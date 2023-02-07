use std::{process::Command, str::from_utf8};

use anyhow::Context;
use clap::Parser;
use dagyo::{
    config::Opts,
    docker::build_docker_image,
    flow::Proc,
    kubestuff,
    vertspec::{Progdef, VertSpec},
};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();

    let vertspecs = VertSpec::from_file(&opts.vertspec).context("loading vertspec")?;

    // build docker images for each progdef
    let mut verts = Vec::new();
    for (name, spec) in vertspecs {
        info!("building docker image for {}", name);
        let hash = build_docker_image(&spec)
            .await
            .context("building docker image")?;
        verts.push(Progdef { spec, hash, name });
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
            let image_name = vert.hash.image_name();
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
    let proc = Proc {
        nodes: vec![
            "source".into(),
            "greet".into(),
            "greet".into(),
            "void_sink".into(),
        ],
        edges: vec![
            ((0, "src".into()), (1, "name".into())),
            ((1, "greeting".into()), (2, "name".into())),
            ((2, "greeting".into()), (3, "sink".into())),
        ],
    };
    let flow = proc.as_graph(&verts)?.with_pipes()?.flow();
    // flow.upload(&cluster).await?;

    // read from the output and print it

    Ok(())
}
