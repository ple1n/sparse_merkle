// Web of trust

#![allow(clippy::large_enum_variant)]

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Add;

use ordermap::OrderMap;
use ordermap::OrderSet;
use petgraph::algo;
use petgraph::algo::k_shortest_path;
use petgraph::algo::min_spanning_tree;
use petgraph::algo::Measure;
use petgraph::data::FromElements;
use petgraph::graph;
use petgraph::visit;
use petgraph::visit::IntoEdgesDirected;
use petgraph::visit::IntoNeighborsDirected;
use petgraph::visit::NodeRef;
use sp1_zkvm::lib::{self, verify::verify_sp1_proof};

/// Same as Node, but with some data hidden by ZKP
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct NodeProof {
    public: Attestation,
    proof: MultiProof,
}

/// Only proofs conforming to the standard public value can be accepted
/// Such a proof only commits once, with this struct.
pub struct StandardPublicValue<NodeIx = MultiHash> {
    // Common parameters
    nodes: BTreeMap<NodeIx, IdentityPub>,
    attest: Vec<Attestation>,
    // output-specific public parameters
    methods: Methods<NodeIx>,
    output: Output,
}

pub enum Methods<NodeIx = MultiHash> {
    /// Simplest method, where the score is derived from weighted whitelists and blacklists
    Weighted { weight: BTreeMap<NodeIx, u32> },
    /// Web of trust
    Web { roots: BTreeMap<NodeIx, u32> },
}

pub enum Output {
    Pending,
    Score(u32),
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Attestation {
    owner: IdentityPub,
    weighted: BTreeMap<IdentityPub, u32>,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum IdentityPub {
    /// Classical way, representing an identity as a key pair
    Publickey([u8; 32]),
    /// Representing identity as knowledge about a hash pre-image
    Hash([u8; 32]),
}

pub enum MultiHash {
    Sha3_256([u8; 32]),
    /// For future use
    Poseidon,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum MultiProof {
    Signature(Vec<u8>),
    Hash(HashOwnership),
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum HashOwnership {
    ZKExternal { vk: [u32; 8], pv: [u8; 32] },
    ZK { pre_image: [u32; 8] },
}

pub struct OwnershipProofs {
    map: BTreeMap<IdentityPub, MultiProof>,
}

use petgraph::Graph;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Node {
    score: u32,
    proof: NodeProof,
}

/// Computed weight as a fraction of total weight
pub struct Weight {
    fraction: u32,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct WeightRuntime {
    fraction: u32,
}

pub type GraphIx = petgraph::graph::DefaultIx;
// pub type Web = Graph<Node, Weight, Directed, GraphIx>;
pub type RuntimeWeb = StableGraph<Node, WeightRuntime, Directed, GraphIx>;

/// Runtime state. Therefore indexed as much as possible.
pub struct ProofWeb {
    web: RuntimeWeb,
    owned: BTreeMap<GraphIx, IdentityPub>,
    proofs: OwnershipProofs,
    public: StandardPublicValue<GraphIx>,
}

use petgraph::prelude::*;
use petgraph::visit::Walker;

// Accepts a partial graph
pub fn compute<V: NodeVerify>(mut proving: ProofWeb, verify: V) {
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut next: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut visited: BTreeSet<NodeIndex> = Default::default();

    match proving.public.methods {
        Methods::Weighted { weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(ix);
                proving.web[node].score = w;
            }
            loop {
                if !this.is_empty() {
                    for (ix, _) in this {
                        if !visited.insert(ix) {
                            continue;
                        }
                        let node = &proving.web[ix];
                        verify.verify_node(&node.proof);

                        let ixes: Vec<_> = proving
                            .web
                            .edges_directed(ix, Direction::Outgoing)
                            .map(|e| (e.id(), e.target()))
                            .collect();
                        let div = ixes.len() as u32;
                        let this_score = node.score;
                        for (e, n) in ixes {
                            let add = this_score / div;
                            proving.web[e].fraction = add;
                            proving.web[n].score += add;
                            next.insert(n, ());
                        }
                    }
                } else {
                    break;
                }
                this = next;
                next = Default::default();
            }
        }
        _ => unimplemented!(),
    }
}

pub trait NodeVerify {
    fn verify_node(&self, _node: &NodeProof) {}
}

pub struct MockVerify;

impl NodeVerify for MockVerify {}

pub type ConstructWeb = Graph<ConstructNode, WeightRuntime, Directed, GraphIx>;
pub enum ConstructNode {
    Root,
    Actual(Node),
}

#[derive(PartialEq, PartialOrd)]
pub struct PathWeight {
    approx: f32,
}

impl Add for PathWeight {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            approx: self.approx.min(rhs.approx),
        }
    }
}

fn construct_search(p: &ProofWeb) {
    // executed outside zkvm
    // construct a web from usable materials
    // find an optimal partial graph for proving to maximize trust score
    let mut con = ConstructWeb::new();
    for n in p.web.node_weights() {
        con.add_node(ConstructNode::Actual(n.to_owned()));
    }

    // k_shortest_path

    todo!()
}

/// Traverses the full graph, and find relevant sub graph
pub fn construct<V: NodeVerify>(mut proving: ProofWeb, verify: V) -> ProofWeb {
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut next: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut visited: BTreeSet<NodeIndex> = Default::default();

    match &mut proving.public.methods {
        Methods::Weighted { weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(*ix);
                proving.web[node].score = *w;
            }
            loop {
                if !this.is_empty() {
                    for (ix, _) in this {
                        if !visited.insert(ix) {
                            continue;
                        }
                        let node = &proving.web[ix];
                        verify.verify_node(&node.proof);

                        let ixes: Vec<_> = proving
                            .web
                            .edges_directed(ix, Direction::Outgoing)
                            .map(|e| (e.id(), e.target()))
                            .collect();
                        let div = ixes.len() as u32;
                        let this_score = node.score;
                        for (e, n) in ixes {
                            let add = this_score / div;
                            proving.web[e].fraction = add;
                            proving.web[n].score += add;
                            next.insert(n, ());
                        }
                    }
                } else {
                    break;
                }
                this = next;
                next = Default::default();
            }
        }
        _ => unimplemented!(),
    }

    // Back track the paths
    let mut prn = RuntimeWeb::new();
    for (ix, key) in &proving.owned {
        insert_max((*ix).into(), &mut proving.web, &mut prn);
    }

    ProofWeb {
        web: prn,
        ..proving
    }
}

pub fn insert_max(pointed: NodeIndex, proving: &mut RuntimeWeb, prn: &mut RuntimeWeb) {
    let src = proving.edges_directed(pointed, Direction::Incoming);
    let max = src.max_by_key(|k| k.weight().fraction);
    if let Some(e) = max {
        let node = proving[e.target()].clone();
        prn[e.target()] = node;
        prn[e.id()] = e.weight().to_owned();
        insert_max(e.target(), proving, prn);
    }
}
