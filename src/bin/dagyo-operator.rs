use std::collections::HashMap;
use std::{sync::Arc, time::Duration};

use anyhow::anyhow as ah;
use clap::Parser;
use futures::StreamExt;
use itertools::Itertools;
use k8s_openapi::api::core::v1::Container;
use k8s_openapi::api::core::v1::{ConfigMap, EnvVar, Pod, PodSpec};
use kube::api::{DeleteParams, ListParams, Patch, PatchParams, PostParams};
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

#[derive(Parser, Debug, Clone)]
pub struct Opts {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = Opts::parse();

    let client = Client::try_default().await?;
    let context = Arc::new(client.clone());
    let cmgs = Api::<DagyoCluster>::all(client.clone());
    let cms = Api::<ConfigMap>::all(client);
    Controller::new(cmgs, watcher::Config::default())
        .owns(cms, watcher::Config::default())
        .run(DagyoCluster::reconcile, DagyoCluster::error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok(o) => println!("reconciled {:?}", o),
                Err(e) => println!("reconcile failed: {:?}", e),
            }
        })
        .await;
    Ok(())
}

// const MB_HOSTNAME: &str = "dagyo-message-broker";
const MB_URL: &str = "amqp://guest:guest@dagyo-message-broker:5672";
// const MB_PORT: u16 = 5672;

#[derive(Error, Debug)]
enum DagyoError {
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),
    #[error("other error: {0}")]
    Other(#[from] anyhow::Error),
}

/// A custom resource
#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "dagyo.dagyo",
    version = "v1",
    kind = "DagyoCluster",
    namespaced
)]
struct ClusterConfig {
    executors: Vec<Executor>,

    sidecar_image: String,
}

impl DagyoCluster {
    fn target_state(&self) -> Deploy {
        let executors: Vec<Pod> = self
            .spec
            .executors
            .iter()
            .map(|executor| {
                let containers = [
                    Container {
                        image: Some(executor.image.clone()),
                        image_pull_policy: Some("IfNotPresent".to_string()),
                        name: "dagyo-executor".into(),
                        ..Container::default()
                    },
                    Container {
                        image: Some(self.spec.sidecar_image.clone()),
                        image_pull_policy: Some("IfNotPresent".to_string()),
                        name: "dagyo-sidecar".into(),
                        env: Some(vec![EnvVar {
                            name: "DAGYO_MESSAGE_BROKER".into(),
                            value: Some(MB_URL.into()),
                            ..EnvVar::default()
                        }]),
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
            })
            .collect_vec();

        Deploy {
            pods: executors
                .into_iter()
                .map(|p| (p.metadata.name.clone().unwrap(), p))
                .collect(),
        }
    }

    async fn current_state(&self, cl: &Client) -> Result<Deploy, DagyoError> {
        let mut ret = Deploy {
            pods: Default::default(),
        };
        let uid = self.uid()?;

        for pod in self
            .pod_client(cl)?
            .list(&ListParams::default())
            .await?
            .items
        {
            if !pod
                .metadata
                .owner_references
                .iter()
                .flatten()
                .any(|or| or.uid == uid)
            {
                continue;
            }
            let name = pod
                .metadata
                .name
                .clone()
                .ok_or_else(|| ah!("found a running pod with no name"))?;
            assert!(ret.pods.insert(name, pod).is_none());
        }

        Ok(ret)
    }

    fn namespace(&self) -> Result<&str, DagyoError> {
        self.metadata
            .namespace
            .as_deref()
            .ok_or_else(|| DagyoError::Other(ah!("missing .metadata.namespace")))
    }

    fn uid(&self) -> Result<&str, DagyoError> {
        self.metadata
            .uid
            .as_deref()
            .ok_or_else(|| DagyoError::Other(ah!("missing .metadata.uid")))
    }

    fn pod_client(&self, cl: &Client) -> Result<Api<Pod>, DagyoError> {
        Ok(Api::<Pod>::namespaced(cl.clone(), self.namespace()?))
    }

    fn error_policy(self: Arc<Self>, _: &DagyoError, _ctx: Arc<Client>) -> Action {
        Action::requeue(Duration::from_secs(60))
    }

    async fn reconcile(self: Arc<Self>, ctx: Arc<Client>) -> Result<Action, DagyoError> {
        let pod_client = self.pod_client(&ctx)?;
        let target = self.target_state();
        let actual = self.current_state(&ctx).await?;

        let delta = actual.plan(&target);
        delta.apply(&pod_client).await?;

        Ok(Action::requeue(Duration::from_secs(300)))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Hash, Eq, PartialEq)]
struct Executor {
    image: String,
}

#[derive(Default)]
struct Deploy {
    pods: HashMap<String, Pod>,
}

impl Deploy {
    fn plan(&self, desired: &Deploy) -> Delta {
        let mut delete = Deploy::default();
        for (name, pod) in &self.pods {
            if desired.pods.contains_key(name) {
                continue;
            }
            delete.pods.insert(name.clone(), pod.clone());
        }

        let mut modify = Deploy::default();
        for (name, pod) in &self.pods {
            if let Some(desired_pod) = desired.pods.get(name) {
                if pod != desired_pod {
                    modify.pods.insert(name.clone(), desired_pod.clone());
                }
            }
        }

        let mut create = Deploy::default();
        for (name, pod) in &desired.pods {
            if !self.pods.contains_key(name) {
                create.pods.insert(name.clone(), pod.clone());
            }
        }

        Delta {
            create,
            modify,
            delete,
        }
    }
}

struct Delta {
    delete: Deploy,
    modify: Deploy,
    create: Deploy,
}

impl Delta {
    async fn apply(&self, pc: &Api<Pod>) -> Result<(), DagyoError> {
        // delete
        for name in self.delete.pods.keys() {
            pc.delete(name, &DeleteParams::default()).await?;
        }

        // modify
        for (name, pod) in &self.modify.pods {
            pc.patch(name, &PatchParams::default(), &Patch::Apply(pod))
                .await?;
        }

        // create
        for pod in self.create.pods.values() {
            pc.create(&PostParams::default(), pod).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::read_to_string;

    use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
    use kube::CustomResourceExt;

    use super::*;

    /// Read a stream of values from a yaml stream.
    fn read_yaml<T: serde::de::DeserializeOwned>(yml: &str) -> Vec<T> {
        use serde_yaml::Deserializer;
        Deserializer::from_str(yml)
            .map(|document| T::deserialize(document).unwrap())
            .collect()
    }

    /// The crd definition in k8s-manifiest.yaml must remain up-to-date with the crd generated above.
    #[test]
    fn crd_is_up_to_date() {
        let manifest = read_to_string("./k8s-manifest.yaml").unwrap();
        let crd = read_yaml::<serde_yaml::Value>(&manifest)
            .into_iter()
            .find(|v| v["kind"] == "CustomResourceDefinition")
            .unwrap();

        let found: CustomResourceDefinition = serde_yaml::from_value(crd).unwrap();
        let generated = <DagyoCluster as CustomResourceExt>::crd();

        if found != generated {
            let found_yaml = serde_yaml::to_string(&found).unwrap();
            let generated_yaml = serde_yaml::to_string(&generated).unwrap();
            panic!(
                "crd in k8s-manifest.yaml is out of date. Found:\n{}\nGenerated:\n{}",
                found_yaml, generated_yaml
            );
        }
    }
}
