// Web of trust

#![allow(clippy::large_enum_variant)]

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::default;
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

#[derive(Default)]
/// Only proofs conforming to the standard public value can be accepted
/// Such a proof only commits once, with this struct.
pub struct StandardPublicValue<NodeIx = MultiHash> {
    // Common parameters
    pub nodes: BTreeMap<NodeIx, IdentityPub>,
    pub attest: Vec<Attestation>,
    // output-specific public parameters
    pub methods: Methods<NodeIx>,
    pub output: Output,
}

pub enum Methods<NodeIx = MultiHash> {
    /// Simplest method, where the score is derived from weighted whitelists and blacklists
    Weighted { weight: BTreeMap<NodeIx, u32> },
    /// Web of trust
    Web { roots: BTreeMap<NodeIx, u32> },
}

impl<Ix> Default for Methods<Ix> {
    fn default() -> Self {
        Self::Web {
            roots: Default::default(),
        }
    }
}

#[derive(Default)]
pub enum Output {
    #[default]
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
    Mock,
}

pub enum MultiHash {
    Sha3_256([u8; 32]),
    /// For future use
    Poseidon,
    Mock,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum MultiProof {
    Signature(Vec<u8>),
    Hash(HashOwnership),
    Mock,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum HashOwnership {
    ZKExternal { vk: [u32; 8], pv: [u8; 32] },
    ZK { pre_image: [u32; 8] },
}

#[derive(Default)]
pub struct OwnershipProofs {
    map: BTreeMap<IdentityPub, MultiProof>,
}

use petgraph::Graph;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Node {
    pub score: u32,
    pub proof: Option<NodeProof>,
}

/// Computed weight as a fraction of total weight
pub struct Edge {
    fraction: u32,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct EdgeRuntime {
    pub fraction: u32,
}

#[cfg(feature = "notzk")]
pub mod notzk {
    use rand::{
        distributions::uniform::{SampleBorrow, SampleUniform, UniformSampler},
        Error, Rng,
    };

    use super::*;
    impl SampleUniform for EdgeRuntime {
        type Sampler = WeightSampler;
    }

    pub struct WeightSampler {
        low: EdgeRuntime,
        high: EdgeRuntime,
        include_high: bool,
    }

    impl UniformSampler for WeightSampler {
        type X = EdgeRuntime;

        fn new<B1, B2>(low_b: B1, high_b: B2) -> Self
        where
            B1: SampleBorrow<Self::X> + Sized,
            B2: SampleBorrow<Self::X> + Sized,
        {
            let low = low_b.borrow().to_owned();
            let high = high_b.borrow().to_owned();
            if low > high {
                unreachable!()
            }

            WeightSampler {
                low,
                high,
                include_high: false,
            }
        }

        fn new_inclusive<B1, B2>(low_b: B1, high_b: B2) -> Self
        where
            B1: SampleBorrow<Self::X> + Sized,
            B2: SampleBorrow<Self::X> + Sized,
        {
            let low = low_b.borrow().clone();
            let high = high_b.borrow().clone();
            if low > high {
                unreachable!()
            }

            WeightSampler {
                low,
                high,
                include_high: true,
            }
        }

        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
            EdgeRuntime {
                fraction: rng.sample(if self.include_high {
                    rand::distributions::Uniform::new_inclusive(
                        self.low.fraction,
                        self.high.fraction,
                    )
                } else {
                    rand::distributions::Uniform::new(self.low.fraction, self.high.fraction)
                }),
            }
        }

        fn sample_single<R: rand::Rng + ?Sized, B1, B2>(low: B1, high: B2, rng: &mut R) -> Self::X
        where
            B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
            B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        {
            let uniform: Self = UniformSampler::new(low, high);
            uniform.sample(rng)
        }

        fn sample_single_inclusive<R: rand::Rng + ?Sized, B1, B2>(
            low: B1,
            high: B2,
            rng: &mut R,
        ) -> Self::X
        where
            B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
            B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        {
            let uniform: Self = UniformSampler::new_inclusive(low, high);
            uniform.sample(rng)
        }
    }
}

pub type GraphIx = petgraph::graph::DefaultIx;
// pub type Web = Graph<Node, Weight, Directed, GraphIx>;
pub type RuntimeWeb = StableGraph<Node, EdgeRuntime, Directed, GraphIx>;

/// Runtime state. Therefore indexed as much as possible.
pub struct ProofWeb<'b, N, E>
where
    for<'a> &'a mut Node: From<&'a mut N>,
    for<'a> &'a mut EdgeRuntime: From<&'a mut E>,
    for<'a> &'a Node: From<&'a N>,
    for<'a> &'a EdgeRuntime: From<&'a E>,
{
    pub web: WrappedGraph<'b, N, E>,
    pub owned: BTreeMap<GraphIx, IdentityPub>,
    pub proofs: OwnershipProofs,
    pub public: StandardPublicValue<GraphIx>,
}

pub struct WrappedGraph<'b, N, E>
where
    for<'a> &'a mut Node: From<&'a mut N>,
    for<'a> &'a mut EdgeRuntime: From<&'a mut E>,
    for<'a> &'a Node: From<&'a N>,
    for<'a> &'a EdgeRuntime: From<&'a E>,
{
    g: &'b mut StableGraph<N, E, Directed, GraphIx>,
}

use petgraph::prelude::*;

// Accepts a partial graph
pub fn compute<'b, N, E, V: NodeVerify>(proving: ProofWeb<'b, N, E>, verify: V)
where
    for<'a> &'a mut Node: From<&'a mut N>,
    for<'a> &'a mut EdgeRuntime: From<&'a mut E>,
    for<'a> &'a Node: From<&'a N>,
    for<'a> &'a EdgeRuntime: From<&'a E>,
{
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut next: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut visited: BTreeSet<NodeIndex> = Default::default();

    match proving.public.methods {
        Methods::Web { roots: weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(ix);
                let n: &mut Node = (&mut proving.web.g[node] as &mut N).into();
                n.score = w;
            }
            loop {
                if !this.is_empty() {
                    for (ix, _) in this {
                        if !visited.insert(ix) {
                            continue;
                        }
                        let node: &Node = (&proving.web.g[ix]).into();
                        let this_score = node.score;
                        verify.verify_node(node.proof.as_ref().unwrap());
                        let ixes: Vec<_> = proving
                            .web
                            .g
                            .edges_directed(ix, Direction::Outgoing)
                            .map(|e| (e.id(), e.target()))
                            .collect();
                        let div = ixes.len() as u32;
                        for (e, n) in ixes {
                            let add = this_score / div;
                            let ex: &mut EdgeRuntime = (&mut proving.web.g[e] as &mut E).into();
                            ex.fraction = add;

                            let nn: &mut Node = (&mut proving.web.g[n] as &mut N).into();
                            nn.score += add;
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

pub type ConstructWeb = Graph<ConstructNode, EdgeRuntime, Directed, GraphIx>;
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

/// Traverses the full graph, and find relevant sub graph
pub fn construct<'b, N, E, V: NodeVerify>(
    mut proving: ProofWeb<'b, N, E>,
    verify: V,
    map: &mut impl MapGraph<IxN = NodeIndex, IxE = EdgeIndex>,
) where
    for<'a> &'a mut Node: From<&'a mut N>,
    for<'a> &'a mut EdgeRuntime: From<&'a mut E>,
    for<'a> &'a Node: From<&'a N>,
    for<'a> &'a EdgeRuntime: From<&'a E>,
{
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut next: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut visited: BTreeSet<NodeIndex> = Default::default();

    match &mut proving.public.methods {
        Methods::Web { roots: weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(*ix);
                let n: &mut Node = (&mut proving.web.g[node] as &mut N).into();
                n.score = *w;
            }
            loop {
                if !this.is_empty() {
                    for (ix, _) in this {
                        if !visited.insert(ix) {
                            continue;
                        }
                        let node: &Node = (&proving.web.g[ix]).into();
                        verify.verify_node(node.proof.as_ref().unwrap());

                        let ixes: Vec<_> = proving
                            .web
                            .g
                            .edges_directed(ix, Direction::Outgoing)
                            .map(|e| (e.id(), e.target()))
                            .collect();
                        let div = ixes.len() as u32;
                        let this_score = node.score;
                        for (e, n) in ixes {
                            let add = this_score / div;
                            let ex: &mut EdgeRuntime = (&mut proving.web.g[e] as &mut E).into();
                            ex.fraction = add;
                            let nn: &mut Node = (&mut proving.web.g[n] as &mut N).into();
                            nn.score += add;
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

    for (ix, key) in &proving.owned {
        insert_max::<N, E>((*ix).into(), &mut proving.web.g, map);
    }
}

pub fn insert_max<N, E>(
    pointed: NodeIndex,
    proving: &mut StableGraph<N, E, Directed, GraphIx>,
    map: &mut impl MapGraph<IxN = NodeIndex, IxE = EdgeIndex>,
) where
    for<'a> &'a mut Node: From<&'a mut N>,
    for<'a> &'a mut EdgeRuntime: From<&'a mut E>,
    for<'a> &'a Node: From<&'a N>,
    for<'a> &'a EdgeRuntime: From<&'a E>,
{
    let src = proving.edges_directed(pointed, Direction::Incoming);
    let max = src.max_by_key(|k| {
        let e: &EdgeRuntime = k.weight().into();
        e.fraction
    });
    if let Some(e) = max {
        let node: &Node = (&proving[e.target()] as &N).into();
        map.map_node(e.target(), node);
        map.map_edge(e.id(), e.weight().into());
        insert_max::<N, E>(e.target(), proving, map);
    }
}

pub struct GraphMapDefault;

/// Represents a change to a graph
pub trait MapGraph {
    type IxN;
    type IxE;
    fn map_node(&mut self, node: Self::IxN, new: &Node) -> bool;
    fn map_edge(&mut self, edge: Self::IxE, new: &EdgeRuntime) -> bool;
}
