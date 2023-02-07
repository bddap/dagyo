use std::{collections::BTreeMap, fmt::Debug};

use crate::{
    config::{KubeNamespace, Opts},
    vertspec::Progdef,
};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        core::v1::{
            Container, ContainerPort, EnvVar, Namespace, Pod, PodSpec, PodTemplateSpec, Service,
            ServicePort, ServiceSpec,
        },
    },
    apimachinery::pkg::{apis::meta::v1::LabelSelector, util::intstr::IntOrString},
    NamespaceResourceScope,
};
use kube::{
    api::{DeleteParams, ListParams, Patch, PatchParams},
    core::ObjectMeta,
    Api, Client, Resource,
};
use serde::{de::DeserializeOwned, Serialize};
use tracing::info;

#[derive(Debug, Clone)]
pub struct Cluster {
    namespace: KubeNamespace,
    deployments: Vec<Deployment>,
    pods: Vec<Pod>,
    services: Vec<Service>,
}

const MQ_HOSTNAME: &str = "dagyo-mq";
const MQ_URL: &str = "amqp://guest:guest@dagyo-mq:443";
const MQ_PORT: u16 = 5672;

impl Cluster {
    /// Set the desired state of the cluster. Prune any resources in self.namespace that are not specified in self.
    pub async fn apply(&self) -> anyhow::Result<()> {
        let client = Client::try_default().await?;

        ensure_namespace(&client, &self.namespace).await?;

        self.apply_resources(&client, &self.deployments).await?;
        self.apply_resources(&client, &self.pods).await?;
        self.apply_resources(&client, &self.services).await?;

        self.prune(&client, &self.deployments).await?;
        self.prune(&client, &self.pods).await?;
        self.prune(&client, &self.services).await?;

        Ok(())
    }

    /// # Panics
    ///
    /// Panics if any resource is unnamed.
    async fn apply_resources<K>(&self, client: &Client, complete_set: &[K]) -> anyhow::Result<()>
    where
        K: Clone + DeserializeOwned + Serialize + Debug + Resource<Scope = NamespaceResourceScope>,
        <K as kube::Resource>::DynamicType: Default,
    {
        let api: Api<K> = Api::namespaced(client.clone(), self.namespace.as_str());

        for resource in complete_set {
            assert!(resource.meta().name.is_some(), "resource is unnamed");
        }

        for patch_params in [
            PatchParams {
                dry_run: true,
                force: false,
                field_manager: Some("dagyo".to_string()),
                field_validation: None,
            },
            PatchParams {
                dry_run: false,
                force: true,
                field_manager: Some("dagyo".to_string()),
                field_validation: None,
            },
        ] {
            for resource in complete_set {
                let r: K = resource.clone();
                let name = resource.meta().name.as_ref().unwrap();
                api.patch(name, &patch_params, &Patch::Apply(r)).await?;
            }
        }

        Ok(())
    }

    pub fn from_verts(opts: &Opts, verts: &[Progdef]) -> Self {
        // will likely need to set the hostname field of the pod spec
        // might neet to set file:///home/a/d/dagyo/target/doc/k8s_openapi/api/core/v1/struct.PodSpec.html#structfield.set_hostname_as_fqdn
        // too

        let mut deployments = Vec::new();
        for vert in verts {
            deployments.push(as_deployment(vert, opts));
        }

        let pods = vec![Pod {
            metadata: ObjectMeta {
                name: Some("dagyo-mq-pod".to_string()),
                namespace: Some(opts.namespace.as_str().to_string()),
                labels: Some([("app".to_string(), "dagyo-mq".to_string())].into()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "dagyo-mq-container".to_string(),
                    image: Some("rabbitmq:3.11".to_string()),
                    ports: Some(vec![ContainerPort {
                        container_port: MQ_PORT.into(),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: None,
        }];

        let services = vec![Service {
            metadata: ObjectMeta {
                name: Some(MQ_HOSTNAME.to_string()),
                namespace: Some(opts.namespace.as_str().to_string()),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                selector: Some([("app".to_string(), "dagyo-mq".to_string())].into()),
                ports: Some(vec![ServicePort {
                    port: MQ_PORT.into(),
                    target_port: Some(IntOrString::Int(MQ_PORT.into())),
                    protocol: Some("TCP".to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }];

        Self {
            namespace: opts.namespace.clone(),
            deployments,
            pods,
            services,
        }
    }

    /// # Panics
    ///
    /// Panics if any resource is unnamed.
    // this function needs some work, it doesn't do exactly what is expected yet
    async fn prune<K>(&self, client: &Client, to_keep: &[K]) -> anyhow::Result<()>
    where
        K: Clone + DeserializeOwned + Debug + Resource<Scope = NamespaceResourceScope>,
        <K as kube::Resource>::DynamicType: Default,
    {
        let api: Api<K> = Api::namespaced(client.clone(), self.namespace.as_str());

        for resource in to_keep {
            assert!(resource.meta().name.is_some(), "resource is unnamed");
        }

        let list = api
            .list(&ListParams {
                label_selector: Some("dagyo_will_prune".to_string()),
                ..Default::default()
            })
            .await?;
        let to_delete: Vec<&str> = list
            .iter()
            .map(|r| r.meta().name.as_ref().unwrap().as_str())
            .filter(|name| {
                !to_keep
                    .iter()
                    .any(|r| r.meta().name.as_ref().unwrap() == name)
            })
            .collect();

        if !to_delete.is_empty() {
            info!("pruning resources: {:?}", &to_delete);
        }

        for delete_params in [
            DeleteParams {
                dry_run: true,
                ..Default::default()
            },
            DeleteParams::default(),
        ] {
            for name in &to_delete {
                api.delete(name, &delete_params).await?;
            }
        }

        Ok(())
    }
}

fn as_deployment(vert: &Progdef, opts: &Opts) -> Deployment {
    let env = vec![
        EnvVar {
            name: "DAGYO_JOBS".to_string(),
            value: Some(format!("dagyo_jobs_{}", vert.hash.shorthex())),
            ..Default::default()
        },
        EnvVar {
            name: "DAGYO_QUEUE".to_string(),
            value: Some(MQ_URL.to_string()),
            ..Default::default()
        },
    ];
    let pod_spec = PodSpec {
        containers: vec![Container {
            name: "dagyo-executor".to_string(),
            image: Some(vert.hash.image_name()),
            env: Some(env),
            // images are content-addressed so we don't need to worry about cache invalidationn
            image_pull_policy: Some("IfNotPresent".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let labels: BTreeMap<String, String> = [("dagyo_executor".to_string(), "a".to_string())].into();
    let deployment_spec = DeploymentSpec {
        replicas: Some(2),
        template: PodTemplateSpec {
            metadata: Some(ObjectMeta {
                labels: Some(labels.clone()),
                namespace: Some(opts.namespace.as_str().to_string()),
                ..Default::default()
            }),
            spec: Some(pod_spec),
        },
        selector: LabelSelector {
            match_labels: Some(labels),
            match_expressions: None,
        },
        ..Default::default()
    };
    let deployment = Deployment {
        metadata: ObjectMeta {
            name: Some(as_pod_name(vert.name.as_str())),
            labels: Some([("dago_will_prune".to_string(), "yes".to_string())].into()),
            namespace: Some(opts.namespace.as_str().to_string()),
            ..Default::default()
        },
        spec: Some(deployment_spec),
        status: None,
    };
    deployment
}

/// pod name must be DNS-1123 label
// This is hack.
// We should ensure that all progdef names are valid DNS-1123 labels
// when loading the toml vertspec.
fn as_pod_name(name_for_humans: &str) -> String {
    let mut s: String = name_for_humans
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_numeric() || *c == '-')
        .take(16)
        .collect();
    if !s.chars().next().unwrap_or(' ').is_ascii_lowercase() {
        s = format!("a{}", s);
    };
    if !s.chars().last().unwrap_or(' ').is_ascii_lowercase() {
        s = format!("{}a", s);
    };
    s
}

async fn ensure_namespace(client: &Client, namespace: &KubeNamespace) -> anyhow::Result<()> {
    let not_namespaced: Api<Namespace> = Api::all(client.clone());
    not_namespaced
        .patch(
            namespace.as_str(),
            &PatchParams {
                force: true,
                field_manager: Some("dagyo".to_string()),
                ..Default::default()
            },
            &Patch::Apply(Namespace {
                metadata: ObjectMeta {
                    name: Some(namespace.as_str().to_string()),
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .await?;

    Ok(())
}
