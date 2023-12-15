//! builds progdefs into images with vertspecs

use std::collections::HashMap;
use std::path::PathBuf;

use dagyo::common::{ClusterConfig, DagyoCluster, Executor};
use dagyo::vertspec::ProgName;
use dagyo::{docker::build_docker_image, vertspec::VertSpec};

use clap::Parser;
use kube::core::ObjectMeta;

#[derive(Parser, Debug)]
struct Args {
    /// The path to vertspec.toml
    vertspec: PathBuf,

    /// The name of this instance of the CRD.
    #[clap(long, default_value = "dagyo-cluster")]
    name: String,

    /// Kubernetes namespace to use.
    #[clap(long, default_value = "dagyo-default")]
    namespace: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let verts = VertSpec::from_file(&args.vertspec)?;
    let verts: HashMap<ProgName, Executor> = verts
        .into_iter()
        .map(|(progname, vertspec)| Ok((progname, build_docker_image(vertspec)?)))
        .collect::<anyhow::Result<_>>()?;
    let spec = ClusterConfig {
        executors: verts,
        sidecar_image: "dagyo-sidecar".to_owned(),
    };
    let output = DagyoCluster {
        metadata: ObjectMeta {
            name: Some(args.name),
            namespace: Some(args.namespace),
            ..Default::default()
        },
        spec,
    };
    serde_json::to_writer(std::io::stdout(), &output)?;
    Ok(())
}
