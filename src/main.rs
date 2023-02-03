use std::{collections::HashMap, path::PathBuf};

use clap::Parser;
use dagyo::{docker::build_docker_image, vertspec::VertSpec};

#[derive(Parser, Debug)]
struct Opts {
    /// Path to program definitions
    #[clap(short, long, env = "DAGYO_VERTS")]
    vertspec: PathBuf,

    /// DAGYO_QUEUE can be specified as an environment variable or a flag
    #[clap(short, long, env = "DAGYO_QUEUE")]
    queue: String,
}

async fn inner_main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let vertspecs = VertSpec::from_file(&opts.vertspec)?;

    // build docker images for each progdef
    let mut image_tags = HashMap::new();
    for (name, vs) in vertspecs {
        let image = build_docker_image(&vs).await?;
        image_tags.insert(name, image);
    }
    eprintln!("image tags: {:#?}", &image_tags);

    // and spin up a docker container for each image
    // https://levelup.gitconnected.com/two-easy-ways-to-use-local-docker-images-in-minikube-cd4dcb1a5379

    // manually connect source to greet and greet to some output

    // read from the output and print it

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = inner_main().await {
        eprintln!("{:?}", e);
        std::process::exit(1);
    }
}
