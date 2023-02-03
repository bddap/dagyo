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

// ## Planned Upgrade Strategy:
//
// Later we can add lifecycle hooks to the container definitions that tell progdefs to stop taking new jobs
// then we'll allocate a large termination grace period, on the order of days, to allow any in-progress jobs to complete
// before the pod shuts down.
//
// This method should handle auto-scaledowns too. When a pod is being scaled down, the signall will tell it to stop taking new jobs
// due to the large termination grace period, the pod will be allowed to finish any in-progress jobs before it shuts down.

// ## Plan for secrets management
//
// We'll define a map from Progdef name to a enviroment dictionary. That list will go into the clusters configmap.
// We'll then set each container's `env_from` setting to pull from configmap.

// ## Compute Resource Requirements
//
// Progdef metadata will eventually include a field for compute resource requirements.
// We'll use that to set the `resources` field on the container definition.

// ## Plan for Failure Tolerace
//
// Failures, AKA "Panics", are aggregated for an entire flow. If one vert fails, the entire flow fails.
// This leave the decision of what to do next to the user. Progdefs should be designed to minimize the
// chance of flow-stopping failures. Here are some tips for reducing Panics.
//
// Progdefs that perform IO should use retries when appicable and helpful.
//
// Effective Progdefs with recoverable failure modes should have
// with explicit failure modes as part of the defined interface.
// For example, when a transformation on some data
// might fail, the output type should be Result<T>, not T.
// This output can be piped into, for exaple, an `assert` vert, or an `ignore_errors` vert
// or a `log_errors` vert.
//
// Another potential pattern could be to define an explicit "error" output stream for the progdef.
// This may be a better fit for some circumstances, such as when an progdef has multiple outputs and
// it would be unclear on which stream to report the error.
//
// Of course, when there is an actual show-stopping failure, the flow *should* fail. An
// example of a show-stopping failure is when a progdef with a defined interface recieves
// a value that fails to deserialize. Deserialization errors on specified types should
// be treated as panic-worthy bugs.

// ## Pod Preemption
//
// https://kubernetes.io/docs/concepts/scheduling-eviction/pod-priority-preemption/
//
// Flows are not recoverable so we never want to kill an executor while it's
// running a job. We will be setting a high value for the
// [gracefully termination period](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-termination)
// of executor pods, so pre-emption would not work against these pods in the first place.

// ## Recovery
//
// Once popped from the queue, the state of a dagyo job exist in *only* one place: the
// memory of the executor that is processing the job. This is an intentional design choice.
//
// The ordering of elements within dagyo streams *is* respected.
// Additionally dagyo ensures that not job is processed twice.
// Each job is processed by at most one executor.
// Executors' statefullnes is respected; dagyo will never swap out an executor mid-job.
//
// In an alternate universe. Resumable executors were implemented by tracking the state each
// job. This substantially complicated things for progdef writers. Progdefs needed to be
// be written as state machines with serializable state. While a nice set of abstractions
// might have made this easier, the version of daggo existing in our universe chooses not
// to spend any
// [strangeness budget](https://steveklabnik.com/writing/the-language-strangeness-budget)
// on the issue. Dagyo chooses not to require progdef implementers to write their progdefs
// as state machines.
//
// This design choice is intentional, but do feel welcome to get in touch if you have
// ideas for implementing recoverable jobs.
