use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct VertSpec {
    #[serde(default = "default_dockerfile")]
    pub dockerfile: PathBuf,
    #[serde(default = "default_build_context")]
    pub docker_build_context: PathBuf,
    #[serde(default)]
    pub docker_build_args: HashMap<String, String>,
    #[serde(default)]
    pub inputs: HashMap<String, String>,
    #[serde(default)]
    pub outputs: HashMap<String, String>,
}

impl VertSpec {
    /// read a set of vertspecs from a file
    /// relative paths are considered relative to the vertspec file
    /// paths returned are canonical and absolute
    pub fn from_file(path: &Path) -> anyhow::Result<HashMap<String, Self>> {
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let vertspec_file = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read vertspec file at {}", path.to_string_lossy())
        })?;
        let mut vertspec: HashMap<String, VertSpec> = toml::from_str(&vertspec_file)?;
        for v in vertspec.values_mut() {
            v.make_relative_to(&parent)?;
        }
        Ok(vertspec)
    }

    fn make_relative_to(&mut self, base: &Path) -> anyhow::Result<()> {
        self.dockerfile = base.join(&self.dockerfile).canonicalize()?;
        self.docker_build_context = base.join(&self.docker_build_context).canonicalize()?;
        Ok(())
    }

    /// a hash that uniquely identifies this vertspecs build
    pub fn content_hash(&self, build_context_tarred: &[u8]) -> ProgdefHash {
        let mut hasher = Sha256::new();
        hasher.update(stable_map_hash(&self.docker_build_args));
        hasher.update(stable_map_hash(&self.inputs));
        hasher.update(stable_map_hash(&self.outputs));
        hasher.update(build_context_tarred);
        ProgdefHash {
            hash: hasher.finalize().into(),
        }
    }
}

pub struct ProgdefHash {
    hash: [u8; 32],
}

impl Debug for ProgdefHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProgdefHash({})", self.shorthex())
    }
}

impl ProgdefHash {
    pub fn image_name(&self) -> String {
        // fyi docker specifically rejects 64 character hex strings
        format!("dagyo_{}", self.shorthex())
    }

    pub fn shorthex(&self) -> String {
        self.hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .take(16)
            .collect()
    }
}

/// hash where if map a == map b then hash a == hash b, this ensures that keys are sorted
fn stable_map_hash(map: &HashMap<String, String>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let mut keys: Vec<(&str, &str)> = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    keys.sort();
    let js = serde_json::to_vec(&keys).unwrap();
    hasher.update(js);
    hasher.finalize().into()
}

#[derive(Debug)]
pub struct Built {
    pub spec: VertSpec,
    pub progdef_hash: ProgdefHash,
    pub name_for_humans: String,
}

fn default_build_context() -> PathBuf {
    PathBuf::from(".")
}

fn default_dockerfile() -> PathBuf {
    PathBuf::from("./Dockerfile")
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[greet]
dockerfile = "./multiple/all.dockerfile"
docker_build_context = "./multiple"
docker_build_args.SCRIPT = "./greet.py"
inputs.name = "utf8-string"
outputs.greeting = "utf8-string"

[source]
dockerfile = "./multiple/all.dockerfile"
docker_build_context = "./multiple"
docker_build_args.SCRIPT = "./source.py"
outputs.some_strings = "utf8-string"
"#;

    #[test]
    fn test_deserialize() {
        let spec: HashMap<String, VertSpec> = toml::from_str(EXAMPLE)
            .map_err(|e| eprintln!("{e}"))
            .unwrap();
        println!("{:#?}", spec);

        assert_eq!(
            spec,
            [
                (
                    "greet".into(),
                    VertSpec {
                        dockerfile: "./multiple/all.dockerfile".into(),
                        docker_build_context: "./multiple".into(),
                        docker_build_args: [("SCRIPT".into(), "./greet.py".into())].into(),
                        inputs: [("name".into(), "utf8-string".into())].into(),
                        outputs: [("greeting".into(), "utf8-string".into())].into(),
                    }
                ),
                (
                    "source".into(),
                    VertSpec {
                        dockerfile: "./multiple/all.dockerfile".into(),
                        docker_build_context: "./multiple".into(),
                        docker_build_args: [("SCRIPT".into(), "./source.py".into())].into(),
                        inputs: HashMap::new(),
                        outputs: [("some_strings".into(), "utf8-string".into())].into(),
                    }
                )
            ]
            .into()
        );
    }
}
