// Web of trust

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use ordermap::OrderMap;
use ordermap::OrderSet;
use petgraph::visit::IntoNeighborsDirected;
use sp1_zkvm::lib::{self, verify::verify_sp1_proof};

/// Same as Node, but with some data hidden by ZKP
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Attestation {
    owner: IdentityPub,
    weighted: BTreeMap<IdentityPub, u32>,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum MultiProof {
    Signature(Vec<u8>),
    Hash(HashOwnership),
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HashOwnership {
    ZKExternal { vk: [u32; 8], pv: [u8; 32] },
    ZK { pre_image: [u32; 8] },
}

pub struct OwnershipProofs {
    map: BTreeMap<IdentityPub, MultiProof>,
}

use petgraph::Graph;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Node {
    score: LazyEval,
    proof: NodeProof,
}

/// Computed weight as a fraction of total weight
pub struct Weight {
    fraction: u32,
}

/// Disable this path, when iterating
pub struct WeightRuntime {
    disabled: bool,
}

pub type GraphIx = petgraph::graph::DefaultIx;
pub type Web = Graph<Node, Weight, Directed, GraphIx>;
pub type RuntimeWeb = Graph<Node, WeightRuntime, Directed, GraphIx>;

/// Runtime state. Therefore indexed as much as possible.
pub struct ProofWeb {
    web: RuntimeWeb,
    owned: BTreeMap<GraphIx, IdentityPub>,
    proofs: OwnershipProofs,
    public: StandardPublicValue<GraphIx>,
}

use petgraph::prelude::*;
use petgraph::visit::Walker;

/// Lazily evaluated trust score
// Get functional
// Keeps all intermediates of computation, without a type system
// Yes functionalism can exist without a type system.
// A type system is just a lame automated logic checker that can never go beyond its formal language anyway.
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct LazyEval {
    ops: OrderSet<TrustOp>,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustOp {
    /// For root nodes
    Assign(u32),
    /// Division by outbounds of some node
    Div(NodeIndex),
}

pub fn calc_trust(proving: &ProofWeb, eval: &LazyEval) -> u32 {
    let mut root = *match eval.ops.first() {
        Some(TrustOp::Assign(root)) => root,
        _ => todo!(),
    };
    for op in eval.ops.iter().skip(1) {
        match op {
            TrustOp::Div(ni) => {
                let ni: Vec<_> = proving
                    .web
                    .neighbors_directed(*ni, Direction::Outgoing)
                    .collect();
                if !ni.is_empty() {
                    root /= ni.len() as u32;
                }
            }
            _ => todo!(),
        }
    }
    root
}

// Accepts a partial graph
pub fn compute<V: NodeVerify>(mut proving: ProofWeb, verify: V) {
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut next: BTreeMap<NodeIndex, ()> = BTreeMap::new();

    match proving.public.methods {
        Methods::Weighted { weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(ix);
                proving.web[node].score.ops.insert(TrustOp::Assign(w));
            }
            loop {
                if !this.is_empty() {
                    for (ix, _) in this {
                        let node = &proving.web[ix];
                        verify.verify_node(&node.proof);
                        let ixes: Vec<_> = proving
                            .web
                            .neighbors_directed(ix, Direction::Outgoing)
                            .collect();
                        for recipient in ixes {
                            let e = proving.web.find_edge(ix, recipient).unwrap();
                            if !proving.web[e].disabled {
                                let new = proving.web[recipient].score.ops.insert(TrustOp::Div(ix));
                                proving.web[e].disabled = !new;
                            }
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

pub fn construct() {
    // executed outside zkvm
    // construct a web from usable materials
    // find an optimal partial graph for proving to maximize trust score
}
