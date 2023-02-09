use std::{fmt::Debug, fmt::Formatter, path::PathBuf, str::FromStr};

use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct Opts {
    /// Path to program definitions
    #[clap(short, long, env = "DAGYO_VERTS")]
    pub vertspec: PathBuf,

    /// Kubernetes namespace to use for this deployment, multiple instances of dagyo can
    /// be run on the same cluster as long as they use different namespaces.
    #[clap(short, long, env = "DAGYO_KUBE_NAMESPACE", default_value = "dagyo")]
    pub namespace: KubeNamespace,

    /// Whether we are running everything locally.
    #[clap(short, long, env = "DAGYO_LOCAL", default_value = "false")]
    pub local: bool,
}

/// Valid custom namespaces:
///   must not be empty, use "default" instead
///   contain at most 63 characters
///   contain only lowercase alphanumeric characters or '-'
///   start with an alphanumeric character
///   end with an alphanumeric character
///   should not start with 'kube-'
#[derive(Clone)]
pub struct KubeNamespace(String);

impl KubeNamespace {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for KubeNamespace {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(!s.is_empty(), "namespace cannot be an empty string");
        anyhow::ensure!(s.len() <= 63, "namespace must be <= 63 characters");
        anyhow::ensure!(
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "namespace must contain only lowercase alphanumeric characters or '-'"
        );
        anyhow::ensure!(
            s.chars()
                .next()
                .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                .expect("already checked that string is not empty"),
            "namespace must start with an alphanumeric character"
        );
        anyhow::ensure!(
            s.chars()
                .last()
                .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                .expect("already checked that string is not empty"),
            "namespace must end with an alphanumeric character"
        );
        anyhow::ensure!(
            !s.starts_with("kube-"),
            "namespace should not start with 'kube-'"
        );
        Ok(Self(s.to_string()))
    }
}

impl Debug for KubeNamespace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
