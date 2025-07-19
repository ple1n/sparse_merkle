// Web of trust

#![allow(clippy::large_enum_variant)]

use std::collections::btree_map;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::default;
use std::ops::Add;

use ark_std::Zero;
use fraction::BigFraction;
use fraction::Fraction;
use itertools::Itertools;
use ordered_float::OrderedFloat;
use ordermap::OrderMap;
use ordermap::OrderSet;
use petgraph::algo;
use petgraph::algo::k_shortest_path;
use petgraph::algo::min_spanning_tree;
use petgraph::algo::Measure;
use petgraph::data::FromElements;
use petgraph::graph;
use petgraph::visit;
use petgraph::visit::IntoEdgeReferences;
use petgraph::visit::IntoEdgesDirected;
use petgraph::visit::IntoNeighborsDirected;
use petgraph::visit::NodeRef;

use serde::Deserialize;
use serde::Serialize;
use sp1_zkvm::lib::{self, verify::verify_sp1_proof};

pub type Fr = OrderedFloat<f32>;

/// Same as Node, but with some data hidden by ZKP
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub struct NodeProof {
    public: Attestation,
    proof: MultiProof,
}

#[derive(Default, Serialize, Deserialize)]
/// Only proofs conforming to the standard public value can be accepted
/// Such a proof only commits once, with this struct.
pub struct StandardPublicValue<NodeIx = MultiHash>
where
    NodeIx: Ord,
{
    // Common parameters
    pub nodes: BTreeMap<NodeIx, IdentityPub>,
    pub attest: Vec<Attestation>,
    // output-specific public parameters
    pub methods: Methods<NodeIx>,
    pub output: Output,
}

#[derive(Serialize, Deserialize)]
pub enum Methods<NodeIx = MultiHash>
where
    NodeIx: Ord,
{
    /// Simplest method, where the score is derived from weighted whitelists and blacklists
    Weighted { weight: BTreeMap<NodeIx, u32> },
    /// Web of trust
    Web { roots: BTreeMap<NodeIx, Fr> },
}

impl<Ix> Default for Methods<Ix>
where
    Ix: Ord,
{
    fn default() -> Self {
        Self::Web {
            roots: Default::default(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub enum Output {
    #[default]
    Pending,
    Score(u32),
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub struct Attestation {
    owner: IdentityPub,
    weighted: BTreeMap<IdentityPub, u32>,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum IdentityPub {
    /// Classical way, representing an identity as a key pair
    Publickey([u8; 32]),
    /// Representing identity as knowledge about a hash pre-image
    Hash([u8; 32]),
    Mock,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum MultiHash {
    Sha3_256([u8; 32]),
    /// For future use
    Poseidon,
    Mock,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum MultiProof {
    Signature(Vec<u8>),
    Hash(HashOwnership),
    Mock,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum HashOwnership {
    ZKExternal { vk: [u32; 8], pv: [u8; 32] },
    ZK { pre_image: [u32; 8] },
}

#[derive(Default, Serialize, Deserialize)]
pub struct OwnershipProofs {
    map: BTreeMap<IdentityPub, MultiProof>,
}

use petgraph::Graph;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub struct Node {
    pub score: Fr,
    pub proof: Option<NodeProof>,
}

/// Computed weight as a fraction of total weight
#[derive(Serialize, Deserialize)]
pub struct Edge {
    fraction: u32,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub struct EdgeRuntime {
    pub fraction: Fr,
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
            let f: f32 = rng.sample(if self.include_high {
                rand::distributions::Uniform::new_inclusive(*self.low.fraction, *self.high.fraction)
            } else {
                rand::distributions::Uniform::new(*self.low.fraction, *self.high.fraction)
            });
            EdgeRuntime {
                fraction: Fr::from(f),
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
pub struct ProofWeb<'b, N, E, C: Conv<Node = N, Edge = E>> {
    pub web: WrappedGraph<'b, N, E, C>,
    pub owned: BTreeMap<GraphIx, IdentityPub>,
    pub proofs: OwnershipProofs,
    pub public: StandardPublicValue<GraphIx>,
}

pub trait Conv: Default {
    type Node;
    type Edge;
    fn node_ref(node: &Self::Node) -> &Node;
    fn edge_ref(edge: &Self::Edge) -> &EdgeRuntime;
    fn node_mut(node: &mut Self::Node) -> &mut Node;
    fn edge_mut(edge: &mut Self::Edge) -> &mut EdgeRuntime;
}

impl<'b, N, E, C: Conv<Node = N, Edge = E>> ProofWeb<'b, N, E, C> {
    pub fn new(g: &'b mut StableGraph<N, E, Directed, GraphIx>, conv: C) -> Self {
        Self {
            web: WrappedGraph { g, conv },
            owned: Default::default(),
            proofs: Default::default(),
            public: Default::default(),
        }
    }
}

pub struct WrappedGraph<'b, N, E, C: Conv<Node = N, Edge = E>> {
    pub g: &'b mut StableGraph<N, E, Directed, GraphIx>,
    pub conv: C,
}

use petgraph::prelude::*;

// Accepts a partial graph
pub fn compute<'b, N, E, V: NodeVerify, C: Conv<Node = N, Edge = E>>(
    proving: ProofWeb<'b, N, E, C>,
    verify: V,
) {
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut next: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    let mut visited: BTreeSet<NodeIndex> = Default::default();

    match proving.public.methods {
        Methods::Web { roots: weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(ix);
                let n: &mut Node = C::node_mut(&mut proving.web.g[node]);
            }
            loop {
                if !this.is_empty() {
                    for (ix, _) in this {
                        if !visited.insert(ix) {
                            continue;
                        }
                        let node: &Node = C::node_ref(&proving.web.g[ix]);
                        let this_score = &node.score;
                        verify.verify_node(node.proof.as_ref().unwrap());
                        let ixes: Vec<_> = proving
                            .web
                            .g
                            .edges_directed(ix, Direction::Outgoing)
                            .map(|e| (e.id(), e.target()))
                            .collect();
                        // let div = ixes.len() as u32;
                        // for (e, n) in ixes {
                        //     let add = this_score / div;
                        //     let ex: &mut EdgeRuntime = C::edge_mut(&mut proving.web.g[e]);
                        //     ex.fraction = add;

                        //     let nn: &mut Node = C::node_mut(&mut proving.web.g[n]);
                        //     nn.score += add;
                        //     next.insert(n, ());
                        // }
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
pub fn construct<'b, N, E, V: NodeVerify, C: Conv<Node = N, Edge = E>>(
    mut proving: ProofWeb<'b, N, E, C>,
    verify: &V,
    map: &mut impl MapGraph<IxN = NodeIndex, IxE = EdgeIndex, S = StableGraph<N, E, Directed, GraphIx>>,
) {
    let mut this: BTreeMap<NodeIndex, ()> = BTreeMap::new();
    match &mut proving.public.methods {
        Methods::Web { roots: weight } => {
            for (ix, w) in weight {
                let node = NodeIndex::from(*ix);
                let n: &mut Node = C::node_mut(&mut proving.web.g[node]);
                this.insert(node, ());
            }
            for (n, _) in this {
                recurse(&mut proving, verify, map, Default::default(), n);
            }
        }
        _ => unimplemented!(),
    }

    // for (ix, key) in &proving.owned {
    //     insert_max::<N, E, C>((*ix).into(), proving.web.g, map, &proving.web.conv);
    // }
}

pub fn recurse<'b, N, E, V: NodeVerify, C: Conv<Node = N, Edge = E>>(
    proving: &mut ProofWeb<'b, N, E, C>,
    verify: &V,
    map: &mut impl MapGraph<IxN = NodeIndex, IxE = EdgeIndex, S = StableGraph<N, E, Directed, GraphIx>>,
    mut path: Vec<NodeIndex>,
    cursor: NodeIndex,
) {
    println!("{:?} -> {:?}", &path, &cursor);
    if let Some(_) = path.iter().find_position(|x| **x == cursor) {
        return;
    }
    let node = C::node_ref(&proving.web.g[cursor]);
    path.push(cursor);
    let ixes: Vec<_> = proving
        .web
        .g
        .edges_directed(cursor, Direction::Outgoing)
        .map(|e| (e.id(), e.target()))
        .collect();
    let this_score = node.score.clone();
    let mut total_frac = Fr::zero();
    for (e, n) in ixes.clone() {
        let ex: &mut EdgeRuntime = C::edge_mut(&mut proving.web.g[e]);
        total_frac += ex.fraction;
    }
    for (e, n) in ixes {
        let ex: &mut EdgeRuntime = C::edge_mut(&mut proving.web.g[e]);
        let add = this_score.clone() * (Fr::from(ex.fraction) / total_frac.clone());
        let nn: &mut Node = C::node_mut(&mut proving.web.g[n]);
        nn.score += add;
        map.map_edge(proving.web.g, e, None);
        map.map_node(proving.web.g, n, None);
        recurse(proving, verify, map, path.clone(), n);
    }
}

pub fn insert_max<N, E, C: Conv<Node = N, Edge = E>>(
    pointed: NodeIndex,
    proving: &mut StableGraph<N, E, Directed, GraphIx>,
    map: &mut impl MapGraph<IxN = NodeIndex, IxE = EdgeIndex, S = StableGraph<N, E, Directed, GraphIx>>,
    conv: &C,
) {
    let src = proving.edges_directed(pointed, Direction::Incoming);
    let max = src
        .max_by_key(|k| {
            let e: &EdgeRuntime = C::edge_ref(k.weight());
            e.fraction
        })
        .map(|e| e.id());
    if map.map_node(proving, pointed, None) {
        return;
    }
    if let Some(e) = max {
        let (src, target) = proving.edge_endpoints(e).unwrap();
        assert_eq!(target, pointed);
        map.map_node(proving, src, None);
        map.map_edge(proving, e, None);
        insert_max::<N, E, C>(src, proving, map, conv);
    }
}

/// Generates a new graph
pub struct GraphMapDefault<S> {
    new: S,
    v_nodes: BTreeMap<NodeIndex, NodeIndex>,
    v_edges: BTreeMap<EdgeIndex, EdgeIndex>,
}

/// Represents a change to a graph. ie. creating a new graph
pub trait MapGraph {
    type IxN;
    type IxE;
    type S;
    /// True, if the node exists
    /// Data is still written for repeated node writes
    fn map_node(&mut self, sg: &mut Self::S, node: Self::IxN, new: Option<Node>) -> bool;
    fn map_edge(&mut self, sg: &mut Self::S, edge: Self::IxE, new: Option<EdgeRuntime>) -> bool;
}

impl<N, E> MapGraph for GraphMapDefault<StableGraph<N, E, Directed, GraphIx>>
where
    Node: Into<N>,
    EdgeRuntime: Into<E>,
    N: Clone,
    E: Clone,
{
    type S = StableGraph<N, E, Directed, GraphIx>;
    type IxE = EdgeIndex;
    type IxN = NodeIndex;
    fn map_node(&mut self, sg: &mut Self::S, node: Self::IxN, new: Option<Node>) -> bool {
        match self.v_nodes.entry(node) {
            btree_map::Entry::Vacant(n) => {
                let i = self
                    .new
                    .add_node(new.map_or_else(|| sg[node].clone(), |x| x.into()));
                n.insert(i);
                false
            }
            btree_map::Entry::Occupied(_) => true,
        }
    }
    fn map_edge(&mut self, sg: &mut Self::S, edge: Self::IxE, new: Option<EdgeRuntime>) -> bool {
        match self.v_edges.entry(edge) {
            btree_map::Entry::Vacant(n) => {
                let (a, b) = sg.edge_endpoints(edge).unwrap();
                let i = self
                    .new
                    .add_edge(a, b, new.map_or_else(|| sg[edge].clone(), |x| x.into()));
                n.insert(i);
                false
            }
            btree_map::Entry::Occupied(_) => true,
        }
    }
}
