use std::collections::HashMap;

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

use crate::vertspec::Progdef;

/// A dag representing a not-yet-created flow.
pub struct Stage0 {
    pub graph: DiGraph<Progdef, (String, String)>,
}

/// A Procedure ready to be executed. The edges of this graph represent
/// freshly generated, not yet initialized, mailboxes for streams.
pub struct Stage1 {
    pub graph: Graph<Progdef, (String, MailBox, String)>,
}

impl Stage1 {
    pub fn from_procedure(procedure: Stage0) -> anyhow::Result<Self> {
        for node in procedure.graph.node_indices() {
            let progdef = procedure.graph.node_weight(node).unwrap();

            // verify all inputs have exactly one source.
            for name in progdef.spec.inputs.keys() {
                let sources = procedure
                    .graph
                    .edges_directed(node, Incoming)
                    .filter(|edge| &edge.weight().1 == name)
                    .count();
                ensure!(
                    sources == 1,
                    "input {} of {} has {} sources, expected 1",
                    name,
                    progdef.name_for_humans,
                    sources,
                );
            }

            // Verify all outputs are source to exactly one input.
            for name in progdef.spec.outputs.keys() {
                let sinks = procedure
                    .graph
                    .edges_directed(node, Outgoing)
                    .filter(|edge| &edge.weight().0 == name)
                    .count();
                ensure!(
                    sinks == 1,
                    "output {} of {} has {} sinks, expected 1",
                    name,
                    progdef.name_for_humans,
                    sinks,
                );
            }
        }

        // Note on type future implementation of inference and template parameters:
        //   We'll assert that if a node has no inputs then all it's outputs are concrete,
        //   we'll use that to infer the types of syncs of those nodes.
        //   Next we let the types propagate through until every node has concretly
        //   typed outputs.
        //   Remember that concrete inputs can imply the type of other inputs.
        // Need some typing dsl to make this work.

        // verify the output typenames are equal input typenames
        for edge in procedure.graph.edge_references() {
            let src = procedure.graph.node_weight(edge.source()).unwrap();
            let dst = procedure.graph.node_weight(edge.target()).unwrap();
            let (outname, inname) = edge.weight();

            ensure!(
                src.spec.outputs.get(outname).unwrap() == dst.spec.inputs.get(inname).unwrap(),
                "output {} of {} does not match input {} of {}",
                outname,
                src.name_for_humans,
                inname,
                dst.name_for_humans
            );
        }

        Ok(Self {
            graph: procedure.graph.map(
                |_, progdef| progdef.clone(),
                |_, (src, sink)| (src.clone(), MailBox::new(), sink.clone()),
            ),
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MailBox {
    pub name: Uuid,
}

impl MailBox {
    pub fn new() -> Self {
        Self {
            name: Uuid::new_v4(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct JobDesc {
    inputs: HashMap<String, MailBox>,
    outputs: HashMap<String, MailBox>,
    panic: MailBox,
    health: MailBox,
    stop: MailBox,
}

impl JobDesc {
    fn from_s1(s1: &Stage1, node: NodeIndex) -> Self {
        let inputs = s1
            .graph
            .edges_directed(node, Incoming)
            .map(|edge| {
                let (_src, mailbox, sink) = edge.weight();
                (sink.clone(), mailbox.clone())
            })
            .collect();
        let outputs = s1
            .graph
            .edges_directed(node, Outgoing)
            .map(|edge| {
                let (src, mailbox, _sink) = edge.weight();
                (src.clone(), mailbox.clone())
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

pub struct Flow {
    pub jobs: Vec<(Progdef, JobDesc)>,
}

impl Flow {
    pub fn from_stage1(s1: Stage1) -> Self {
        let jobs = s1
            .graph
            .node_indices()
            .map(|node| {
                let progdef = s1.graph.node_weight(node).unwrap().clone();
                (progdef, JobDesc::from_s1(&s1, node))
            })
            .collect();
        Flow { jobs }
    }
}
