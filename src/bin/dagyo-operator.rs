use std::{collections::HashSet, sync::Arc, time::Duration};

use futures::StreamExt;
use itertools::Itertools;
use k8s_openapi::api::core::v1::Container;
use k8s_openapi::api::core::v1::{ConfigMap, Pod, PodSpec};
use kube::api::{Patch, PatchParams};
use kube::{
    core::ObjectMeta,
    runtime::{
        controller::{Action, Controller},
        watcher,
    },
};
use kube::{Api, Client, CustomResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
enum DagyoError {
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),
    #[error("other error: {0}")]
    Other(#[from] anyhow::Error),
}

/// A custom resource
#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(group = "dagyo", version = "v1", kind = "DagyoCluster", namespaced)]
struct ClusterConfig {
    executors: HashSet<Executor>,
    sidecar_image: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Hash, Eq, PartialEq)]
struct Executor {
    image: String,
}

fn error_policy(_: Arc<DagyoCluster>, _: &DagyoError, _ctx: Arc<Client>) -> Action {
    Action::requeue(Duration::from_secs(60))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let context = Arc::new(client.clone());
    let cmgs = Api::<DagyoCluster>::all(client.clone());
    let cms = Api::<ConfigMap>::all(client);
    Controller::new(cmgs, watcher::Config::default())
        .owns(cms, watcher::Config::default())
        .run(reconcile, error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok(o) => println!("reconciled {:?}", o),
                Err(e) => println!("reconcile failed: {:?}", e),
            }
        })
        .await;
    Ok(())
}

async fn reconcile(dc: Arc<DagyoCluster>, ctx: Arc<Client>) -> Result<Action, DagyoError> {
    let namespace = dc
        .metadata
        .namespace
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing .metadata.namespace"))?;
    let name = dc
        .metadata
        .name
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing .metadata.name"))?;
    let client = Api::<Pod>::namespaced(Client::clone(&ctx), namespace);

    let pod_for = |executor: &Executor| -> Pod {
        let containers = [
            Container {
                image: Some(executor.image.clone()),
                name: "executor".into(),
                ..Container::default()
            },
            Container {
                image: Some(dc.spec.sidecar_image.clone()),
                name: "sidecar".into(),
                ..Container::default()
            },
        ]
        .to_vec();
        Pod {
            metadata: ObjectMeta {
                name: Some(executor.image.clone()),
                ..ObjectMeta::default()
            },
            spec: Some(PodSpec {
                containers,
                ..PodSpec::default()
            }),
            ..Pod::default()
        }
    };

    let pods = dc.spec.executors.iter().map(pod_for).collect_vec();
    client
        .patch(name, &PatchParams::default(), &Patch::Apply(&pods))
        .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}
