// #[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
// pub struct VertSpec {
//     #[serde(default = "default_dockerfile")]
//     pub dockerfile: PathBuf,
//     #[serde(default = "default_build_context")]
//     pub docker_build_context: PathBuf,
//     #[serde(default)]
//     pub docker_build_args: HashMap<String, String>,
//     #[serde(default)]
//     pub inputs: HashMap<String, String>,
//     #[serde(default)]
//     pub outputs: HashMap<String, String>,
// }

// impl VertSpec {
//     /// read a set of vertspecs from a file
//     /// relative paths are considered relative to the vertspec file
//     /// paths returned are canonical and absolute
//     pub fn from_file(path: &Path) -> anyhow::Result<HashMap<String, Self>> {
//         let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
//         let vertspec_file = std::fs::read_to_string(&path).with_context(|| {
//             format!("failed to read vertspec file at {}", path.to_string_lossy())
//         })?;
//         let mut vertspec: HashMap<String, VertSpec> = toml::from_str(&vertspec_file)?;
//         for v in vertspec.values_mut() {
//             v.make_relative_to(&parent)?;
//         }
//         Ok(vertspec)
//     }

//     fn make_relative_to(&mut self, base: &Path) -> anyhow::Result<()> {
//         self.dockerfile = base.join(&self.dockerfile).canonicalize()?;
//         self.docker_build_context = base.join(&self.docker_build_context).canonicalize()?;
//         Ok(())
//     }
// }

// pub type ImageName = String;

// #[derive(Debug)]
// pub struct Built {
//     pub spec: VertSpec,
//     pub image: ImageName,
//     pub name_for_humans: String,
// }

// use kube::api::{Api, ListParams, PostParams};

// fn create_cluster_definition(built_vertspecs: &[Built]) -> anyhow::Result<ClusterDefinition> {
//     let mut cluster_definition = ClusterDefinition::default();
//     for built in built_vertspecs {
//         let mut vert = Vert::default();
//         vert.inputs = built.spec.inputs.clone();
//         vert.outputs = built.spec.outputs.clone();
//         cluster_definition.verts.insert(built.image.clone(), vert);
//     }
//     Ok(cluster_definition)
// }

use crate::{config::Opts, vertspec::Built};
use k8s_openapi::api::{
    apps::v1::{Deployment, DeploymentSpec},
    core::v1::{Container, EnvVar, PodSpec, PodTemplateSpec},
};
use kube::core::ObjectMeta;

pub fn define_cluster(opts: &Opts, verts: &[Built]) -> Vec<Deployment> {
    // let dagyo_mq_hostname = "dagyo-mq";
    // will likely need to set the hostname field of the pod spec
    // might neet to set file:///home/a/d/dagyo/target/doc/k8s_openapi/api/core/v1/struct.PodSpec.html#structfield.set_hostname_as_fqdn
    // too

    let mut ret = Vec::new();
    for vert in verts {
        let metadata = ObjectMeta {
            name: Some(format!("dagyo_executor_{}", vert.progdef_hash.hex())),
            annotations: Some(
                [
                    ("dagyo_progdef_hash".into(), vert.progdef_hash.hex()),
                    ("name_for_human".into(), vert.name_for_humans.clone()),
                ]
                .into(),
            ),
            labels: Some([("dagyo_purpose".to_string(), "dagyo_executor".to_string())].into()),
            namespace: Some(opts.namespace.as_str().to_string()),
            ..Default::default()
        };

        let pod_spec = PodSpec {
            containers: vec![Container {
                name: "dagyo_executor".to_string(),
                image: Some(vert.progdef_hash.image_name()),
                env: Some(vec![
                    EnvVar {
                        name: "DAGYO_JOBS".to_string(),
                        value: Some(format!("dagyo_jobs_{}", vert.progdef_hash.hex())),
                        ..Default::default()
                    },
                    EnvVar {
                        name: "DAGYO_QUEUE".to_string(),
                        value: Some("todo: queue url".to_string()),
                        ..Default::default()
                    },
                ]),
                // Images are content-addressed so no need for cache invalidation.
                image_pull_policy: Some("IfNotPresent".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let deployment = Deployment {
            metadata: metadata.clone(),
            spec: Some(DeploymentSpec {
                replicas: Some(1),
                template: PodTemplateSpec {
                    metadata: Some(metadata),
                    spec: Some(pod_spec),
                },
                ..Default::default()
            }),
            status: None,
        };
        ret.push(deployment);
    }
    ret
}
