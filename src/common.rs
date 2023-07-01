use std::collections::HashMap;

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

type IoName = String;
type Type = String;
type ContainerImage = String;
type ProgName = String;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
pub struct Executor {
    pub image: ContainerImage,
    pub inputs: HashMap<IoName, Type>,
    pub outputs: HashMap<IoName, Type>,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "dagyo.dagyo",
    version = "v1",
    kind = "DagyoCluster",
    namespaced
)]
pub struct ClusterConfig {
    pub executors: HashMap<ProgName, Executor>,
    pub sidecar_image: String,
}
