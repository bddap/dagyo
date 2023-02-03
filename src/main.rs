use clap::Parser;
use dagyo::{
    config::Opts,
    docker::build_docker_image,
    kubestuff,
    vertspec::{Built, VertSpec},
};

async fn inner_main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let vertspecs = VertSpec::from_file(&opts.vertspec)?;

    // build docker images for each progdef
    let mut verts = Vec::new();
    for (name_for_humans, spec) in vertspecs {
        let progdef_hash = build_docker_image(&spec).await?;
        verts.push(Built {
            spec,
            progdef_hash,
            name_for_humans,
        });
    }
    eprintln!("image tags: {:#?}", &verts);

    // and spin up a docker container for each image
    // https://levelup.gitconnected.com/two-easy-ways-to-use-local-docker-images-in-minikube-cd4dcb1a5379
    let _cluster = kubestuff::define_cluster(&opts, &verts);
    // kubestuff::set_cluster(&verts, &opts.queue).await?;
    // let client = kube::Client::try_default().await?;
    // .apply(&kube::api::PostParams::default(), &kube::api::List::new(vec![]))
    // .await?;

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
