use std::{
    collections::{HashMap, HashSet},
    iter::once,
};

use anyhow::ensure;
use petgraph::{
    prelude::DiGraph,
    stable_graph::NodeIndex,
    visit::EdgeRef,
    Direction::{Incoming, Outgoing},
    Graph,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::vertspec::{ProgName, Progdef};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The serializable representation of a procedure.
pub struct Proc {
    pub nodes: Vec<ProgName>,

    /// Which output is connected to which input
    /// the structure of the graph is validated later,
    /// along with type checking.
    pub edges: Vec<((usize, String), (usize, String))>,
}

impl Proc {
    pub fn as_graph(&self, progdefs: &[Progdef]) -> anyhow::Result<Procedure> {
        let mut graph = DiGraph::new();
        let mut idxes = HashMap::new();

        let progdefs: HashMap<String, Progdef> = progdefs
            .iter()
            .map(|progdef| (progdef.name.clone(), progdef.clone()))
            .collect();

        for (i, progname) in self.nodes.iter().enumerate() {
            let progdef = progdefs.get(progname).ok_or_else(|| {
                anyhow::anyhow!("program {} not found in program definitions", progname)
            })?;
            let idx = graph.add_node(progdef.clone());
            idxes.insert(i, idx);
        }

        for ((left_idx, output_name), (right_idx, input_name)) in self.edges.iter() {
            let left_idx = *idxes
                .get(left_idx)
                .ok_or_else(|| anyhow::anyhow!("node {} not found in procedure", left_idx))?;
            let right_idx = *idxes
                .get(right_idx)
                .ok_or_else(|| anyhow::anyhow!("node {} not found in procedure", right_idx))?;

            graph.add_edge(
                left_idx,
                right_idx,
                (output_name.clone(), input_name.clone()),
            );
        }

        Ok(Procedure { graph })
    }
}

/// A dag representing a not-yet-created flow.
pub struct Procedure {
    graph: DiGraph<Progdef, (String, String)>,
}

impl Procedure {
    pub fn with_pipes(self) -> anyhow::Result<WithPipes> {
        self.validate()?;
        Ok(WithPipes {
            graph: self.graph.map(
                |_, progdef| progdef.clone(),
                |_, (src, sink)| (src.clone(), MailBox::new(), sink.clone()),
            ),
        })
    }

    /// Check for validity of the procedure.
    /// No inputs should be unconnected.
    /// No outputs should be unconnected.
    /// Includes type checking.
    fn validate(&self) -> anyhow::Result<()> {
        for node in self.graph.node_indices() {
            let progdef = self.graph.node_weight(node).unwrap();

            // verify all inputs have exactly one source.
            for name in progdef.spec.inputs.keys() {
                let sources = self
                    .graph
                    .edges_directed(node, Incoming)
                    .filter(|edge| &edge.weight().1 == name)
                    .count();
                ensure!(
                    sources == 1,
                    "input {} of {} has {} sources, expected 1",
                    name,
                    progdef.name,
                    sources,
                );
            }

            // verify all outputs have exactly one sink.
            for name in progdef.spec.outputs.keys() {
                let sinks = self
                    .graph
                    .edges_directed(node, Outgoing)
                    .filter(|edge| &edge.weight().0 == name)
                    .count();
                ensure!(
                    sinks == 1,
                    "output {} of {} has {} sinks, expected 1",
                    name,
                    progdef.name,
                    sinks,
                );
            }
        }

        for edge in self.graph.edge_references() {
            let (src, sink) = edge.weight();
            let left = self.graph.node_weight(edge.source()).unwrap();
            let right = self.graph.node_weight(edge.target()).unwrap();

            // verify that the edge pulls from an output that actually exists.
            left.spec.outputs.get(src).ok_or_else(|| {
                anyhow::anyhow!(
                    "program {} does not have an output named {}",
                    left.name,
                    src
                )
            })?;

            // verify that the edge pushes to an input that actually exists.
            right.spec.inputs.get(sink).ok_or_else(|| {
                anyhow::anyhow!(
                    "program {} does not have an input named {}",
                    right.name,
                    sink
                )
            })?;
        }

        // ensure there are no cycles
        petgraph::algo::toposort(&self.graph, None)
            .map_err(|cycle| anyhow::anyhow!("cycle detected: {:?}", cycle))?;

        // Note on type future implementation of inference and template parameters:
        //   We'll assert that if a node has no inputs then all it's outputs are concrete,
        //   we'll use that to infer the types of syncs of those nodes.
        //   Next we let the types propagate through until every node has concretly
        //   typed outputs.
        //   Remember that concrete inputs can imply the type of other inputs.
        // Need some typing dsl to make this work.

        // verify the output typenames are equal input typenames
        for edge in self.graph.edge_references() {
            let src = self.graph.node_weight(edge.source()).unwrap();
            let dst = self.graph.node_weight(edge.target()).unwrap();
            let (outname, inname) = edge.weight();

            ensure!(
                src.spec.outputs.get(outname).unwrap() == dst.spec.inputs.get(inname).unwrap(),
                "output {} of {} does not match input {} of {}",
                outname,
                src.name,
                inname,
                dst.name
            );
        }

        Ok(())
    }
}

/// A Procedure ready to be executed. The edges of this graph represent
/// freshly generated, not yet initialized, mailboxes for streams.
pub struct WithPipes {
    graph: Graph<Progdef, (String, MailBox, String)>,
}

impl WithPipes {
    pub fn flow(&self) -> Flow {
        let jobs = self
            .graph
            .node_indices()
            .map(|node| {
                let progdef = self.graph.node_weight(node).unwrap().clone();
                (progdef, self.job_desc(node))
            })
            .collect();
        Flow { jobs }
    }

    fn job_desc(&self, node: NodeIndex) -> JobDesc {
        let inputs = self
            .graph
            .edges_directed(node, Incoming)
            .map(|edge| {
                let (_src, mailbox, sink) = edge.weight();
                (sink.clone(), *mailbox)
            })
            .collect();
        let outputs = self
            .graph
            .edges_directed(node, Outgoing)
            .map(|edge| {
                let (src, mailbox, _sink) = edge.weight();
                (src.clone(), *mailbox)
            })
            .collect();
        JobDesc {
            inputs,
            outputs,
            panic: MailBox::new(),
            health: MailBox::new(),
            stop: MailBox::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug, Copy)]
/// Adress for executors to push xor pull from.
#[serde(transparent)]
pub struct MailBox {
    addr: Uuid,
}

impl MailBox {
    fn new() -> Self {
        Self {
            addr: Uuid::new_v4(),
        }
    }

    pub fn queue_name(&self) -> String {
        self.addr.to_string()
    }
}

#[derive(Serialize, Deserialize)]
pub struct JobDesc {
    pub inputs: HashMap<String, MailBox>,
    pub outputs: HashMap<String, MailBox>,
    pub panic: MailBox,
    pub health: MailBox,
    pub stop: MailBox,
}

pub struct Flow {
    jobs: Vec<(Progdef, JobDesc)>,
}

impl Flow {
    pub async fn upload(&self, mq: &lapin::Connection) -> anyhow::Result<()> {
        let channel = mq.create_channel().await?;

        let ephemeral_queues: HashSet<&MailBox> = self
            .jobs
            .iter()
            .flat_map(|(_progdef, jobdesc)| {
                once(&jobdesc.panic)
                    .chain(once(&jobdesc.health))
                    .chain(once(&jobdesc.stop))
                    .chain(jobdesc.inputs.values())
                    .chain(jobdesc.outputs.values())
            })
            .collect();

        for p in ephemeral_queues {
            channel
                .queue_declare(
                    &p.queue_name(),
                    lapin::options::QueueDeclareOptions::default(),
                    lapin::types::FieldTable::default(),
                )
                .await?;
        }

        for (progdef, jobdesc) in &self.jobs {
            let message = serde_json::to_vec(&jobdesc).unwrap();
            channel
                .basic_publish(
                    "",
                    &progdef.hash.job_mailbox(),
                    Default::default(),
                    &message,
                    Default::default(),
                )
                .await?;
        }

        Ok(())
    }
}

pub fn from_toml(toml: &str, progdefs: &[Progdef]) -> anyhow::Result<Vec<Flow>> {
    #[derive(Debug, Serialize, Deserialize)]
    struct TomlProcedures {
        procedure: Vec<Proc>,
    }
    let flows: TomlProcedures = toml::from_str(toml).unwrap();
    let mut ret: Vec<Flow> = Vec::new();
    for proc in flows.procedure {
        let f = proc.as_graph(progdefs)?.with_pipes()?.flow();
        ret.push(f);
    }
    Ok(ret)
}
