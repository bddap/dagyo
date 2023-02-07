use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process::Command,
    str::from_utf8,
};

use anyhow::Context;
use clap::Parser;
use dagyo::{
    config::Opts,
    docker::build_docker_image,
    flow::Proc,
    kubestuff,
    vertspec::{Progdef, VertSpec},
};
use futures::future::select;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();

    let vertspecs = VertSpec::from_file(&opts.vertspec).context("loading vertspec")?;

    info!("building docker image for each progdef");
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

    info!("spin up a docker container for each image");
    let cluster = kubestuff::Cluster::from_verts(&opts, &verts);
    cluster.apply().await?;
    info!("spun up containers");

    info!("manually connect source to greet and greet to some output");
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

    info!("connecting to message queue");
    let broker = connect_to_broker(opts.clone()).await?;
    dagyo::queue::ensure_queues(&broker, &verts).await?;

    info!("uploading flow..");
    flow.upload(&broker).await?;
    info!("uploaded flow");

    // read from the output and print it

    // cleanup

    Ok(())
}

async fn tcp_serve_local(tcp_listener: TcpListener, api: Api<Pod>) -> anyhow::Result<()> {
    let mut port_forward = api.portforward("dagyo-mq-pod", &[5672]).await?;
    let stream = port_forward.take_stream(5672).unwrap();
    let (mut tcp_stream, _) = tcp_listener.accept().await?;
    let (mut stream_read, mut stream_write) = tokio::io::split(stream);

    tokio::spawn(async move {
        let (mut tcp_read, mut tcp_write) = tcp_stream.split();
        let res = select(
            Box::pin(tokio::io::copy(&mut tcp_read, &mut stream_write)),
            Box::pin(tokio::io::copy(&mut stream_read, &mut tcp_write)),
        )
        .await
        .factor_first()
        .0
        .context("port forwarding");
        if let Err(e) = res {
            error!("{}", e);
        }
    });

    Ok(())
}

/// create a connection to the message broker over a kubernetes port-forward
/// tunnel. this works by running a local tcp server that forwards traffic
/// through the port-forward to the message broker.
async fn connect_to_broker(opts: Opts) -> anyhow::Result<lapin::Connection> {
    // bind a local port on ipv6 localhost
    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let tcp_listener = TcpListener::bind(socket_addr).await?;
    let actual_addr = tcp_listener.local_addr()?;

    let client = kube::Client::try_default().await?;
    let api: Api<Pod> = Api::namespaced(client, opts.namespace.as_str());

    // spawn the tcp server
    tokio::spawn(async move { tcp_serve_local(tcp_listener, api).await });

    // connect to the local tcp server
    let url = format!("amqp://guest:guest@{}", actual_addr);
    info!("connecting to {}", url);
    let ret = lapin::Connection::connect(&url, lapin::ConnectionProperties::default()).await?;

    Ok(ret)
}
