use std::collections::HashMap;
use std::{sync::Arc, time::Duration};

use anyhow::anyhow as ah;
use clap::Parser;
use futures::StreamExt;
use itertools::Itertools;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    ConfigMap, EnvVar, Pod, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::api::core::v1::{Container, ContainerPort};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{DeleteParams, ListParams, Patch, PatchParams, PostParams};
use kube::{
    core::ObjectMeta,
    runtime::{
        controller::{Action, Controller},
        watcher,
    },
};
use kube::{Api, Client, CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Parser, Debug, Clone)]
pub struct Opts {
    /// Output the yaml crd (custom resource description) for this operator then exit.
    #[clap(long)]
    pub crd: bool,

    /// Output the yaml deployment for this operator then exit.
    #[clap(long, conflicts_with = "crd")]
    pub deployment: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    if opts.crd {
        let crd = <DagyoCluster as CustomResourceExt>::crd();
        serde_yaml::to_writer(std::io::stdout(), &crd)?;
        return Ok(());
    }

    if opts.deployment {
        let deploy = serde_yaml::to_string(&deployment())?;
        println!("{}", deploy);
        return Ok(());
    }

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

fn deployment() -> Deployment {
    let labels = [("app".to_string(), "dagyo-operator".to_string())];
    Deployment {
        metadata: ObjectMeta {
            name: Some("dagyo-operator".to_string()),
            labels: Some(labels.clone().into()),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector {
                match_labels: Some(labels.clone().into()),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels.into()),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "dagyo-operator".to_string(),
                        image: Some("dagyo-operator".to_string()),
                        image_pull_policy: Some("IfNotPresent".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}

const MB_HOSTNAME: &str = "dagyo-message-broker";
const MB_URL: &str = "amqp://guest:guest@dagyo-message-broker:5672";
const MB_PORT: u16 = 5672;

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

    #[serde(default = "default_sidecar_image")]
    sidecar_image: String,

    #[serde(default = "default_scheduler_image")]
    scheduler_image: String,

    /// Image of a message broker supporting the ampq protocol.
    #[serde(default = "default_mb_image")]
    message_broker_image: String,
}

fn default_sidecar_image() -> String {
    "dagyo-sidecar".to_string()
}

fn default_scheduler_image() -> String {
    "dagyo-scheduler".to_string()
}

fn default_mb_image() -> String {
    "rabbitmq:3.11".to_string()
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

        let broker_labels = [("app".to_string(), "dagyo-message-broker".to_string())];
        let broker = Pod {
            metadata: ObjectMeta {
                name: Some("dagyo-message-broker".to_string()),
                labels: Some(broker_labels.clone().into()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "dagyo-message-broker".to_string(),
                    image: Some(self.spec.message_broker_image.clone()),
                    image_pull_policy: Some("IfNotPresent".to_string()),
                    ports: Some(vec![ContainerPort {
                        container_port: MB_PORT.into(),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: None,
        };
        let broker_service = Service {
            metadata: ObjectMeta {
                name: Some(MB_HOSTNAME.to_owned()),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                selector: Some(broker_labels.into()),
                ports: Some(vec![ServicePort {
                    port: MB_PORT.into(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let scheduler_labels = [("app".to_string(), "dagyo-scheduler".to_string())];
        let scheduler = Pod {
            metadata: ObjectMeta {
                name: Some("dagyo-scheduler".to_string()),
                labels: Some(scheduler_labels.clone().into()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "dagyo-scheduler".to_string(),
                    image: Some(self.spec.scheduler_image.clone()),
                    image_pull_policy: Some("IfNotPresent".to_string()),
                    env: Some(vec![EnvVar {
                        name: "DAGYO_MESSAGE_BROKER".into(),
                        value: Some(MB_URL.into()),
                        ..EnvVar::default()
                    }]),
                    ports: Some(vec![ContainerPort {
                        container_port: 80,
                        ..Default::default()
                    }]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: None,
        };
        let scheduler_service = Service {
            metadata: ObjectMeta {
                name: Some("dagyo-scheduler".to_string()),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                selector: Some(scheduler_labels.into()),
                ports: Some(vec![ServicePort {
                    port: 80,
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        Deploy {
            pods: executors
                .into_iter()
                .chain([broker, scheduler])
                .map(|pod| (pod.metadata.name.clone().unwrap(), pod))
                .collect(),
            services: [
                (
                    broker_service.metadata.name.clone().unwrap(),
                    broker_service,
                ),
                (
                    scheduler_service.metadata.name.clone().unwrap(),
                    scheduler_service,
                ),
            ]
            .into(),
        }
    }

    async fn current_state(&self, cl: &Client) -> Result<Deploy, DagyoError> {
        let mut ret = Deploy {
            pods: Default::default(),
            services: Default::default(),
        };
        let uid = self.uid()?;

        for service in self
            .service_client(cl)?
            .list(&ListParams::default())
            .await?
            .items
        {
            if !service
                .metadata
                .owner_references
                .iter()
                .flatten()
                .any(|or| or.uid == uid)
            {
                continue;
            }
            let name = service
                .metadata
                .name
                .clone()
                .ok_or_else(|| ah!("found a running service with no name"))?;
            assert!(ret.services.insert(name, service).is_none());
        }

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

    fn service_client(&self, cl: &Client) -> Result<Api<Service>, DagyoError> {
        Ok(Api::<Service>::namespaced(cl.clone(), self.namespace()?))
    }

    fn error_policy(self: Arc<Self>, _: &DagyoError, _ctx: Arc<Client>) -> Action {
        Action::requeue(Duration::from_secs(60))
    }

    async fn reconcile(self: Arc<Self>, ctx: Arc<Client>) -> Result<Action, DagyoError> {
        let pod_client = self.pod_client(&ctx)?;
        let service_client = self.service_client(&ctx)?;
        let target = self.target_state();
        let actual = self.current_state(&ctx).await?;

        let delta = actual.plan(&target);
        delta.apply(&pod_client, &service_client).await?;

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
    services: HashMap<String, Service>,
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
        for (name, service) in &self.services {
            if desired.services.contains_key(name) {
                continue;
            }
            delete.services.insert(name.clone(), service.clone());
        }

        let mut modify = Deploy::default();
        for (name, pod) in &self.pods {
            if let Some(desired_pod) = desired.pods.get(name) {
                if pod != desired_pod {
                    modify.pods.insert(name.clone(), desired_pod.clone());
                }
            }
        }
        for (name, service) in &self.services {
            if let Some(desired_service) = desired.services.get(name) {
                if service != desired_service {
                    modify
                        .services
                        .insert(name.clone(), desired_service.clone());
                }
            }
        }

        let mut create = Deploy::default();
        for (name, pod) in &desired.pods {
            if !self.pods.contains_key(name) {
                create.pods.insert(name.clone(), pod.clone());
            }
        }
        for (name, service) in &desired.services {
            if !self.services.contains_key(name) {
                create.services.insert(name.clone(), service.clone());
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
    async fn apply(&self, pc: &Api<Pod>, sc: &Api<Service>) -> Result<(), DagyoError> {
        // delete
        for name in self.delete.pods.keys() {
            pc.delete(name, &DeleteParams::default()).await?;
        }
        for name in self.delete.services.keys() {
            sc.delete(name, &DeleteParams::default()).await?;
        }

        // modify
        for (name, pod) in &self.modify.pods {
            pc.patch(name, &PatchParams::default(), &Patch::Apply(pod))
                .await?;
        }
        for (name, service) in &self.modify.services {
            sc.patch(name, &PatchParams::default(), &Patch::Apply(service))
                .await?;
        }

        // create
        for pod in self.create.pods.values() {
            pc.create(&PostParams::default(), pod).await?;
        }
        for service in self.create.services.values() {
            sc.create(&PostParams::default(), service).await?;
        }

        Ok(())
    }
}
