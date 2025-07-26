
= Web of trust, as a fuzzy logic system

There isn't a need to fixate over a specific system of fuzzy logic, but we want such as system to be
+ Resistant to attacks. The system should be resistant to sybils
+ Irreducible. Good systems should not be reducible to a simpler implementation
+ Interpretable. So far I don't want a neuronal approach.
+ Maximally discriminating, the system should fully utilize the input to generate different trust scores.

== Axiomatic vector logic

#set math.vec(delim: "[")

$
  "Axioms" := vec(a, b, c, ...,) = v_A in R^n = 100% dot v_A
$

Axioms are considered irreducible, to each other, and opaque, by definition.

$a, b, c, . ..$ are unit vectors.

Propositions are connected in _conjunctive normal form_.

ie, a list of OR gates connected by an AND gate.

The gates vary depending on the fuzzy logic system you choose.

It means each proposition is represented as a vector of confidence over axioms, an extended form of fuzzy logic.

Logical calculation of each axiom's dimension is independent.

=== Reduction or simplification

Vectors may be reduced in simplified implementation.

For a proposition $p=k dot v_A$, and truthiness of axioms as $t_A in R^n$. Either

1. Take the conjuction of each coefficent axiom pair
2. Take the disjunction

== Example fuzzy logic system

Axioms are treated as normal propositions with assigned values.

Each node represents operations over other propositions.

Input
- AND gate, $min(v_"in")$
- OR gate, $max(v_"in")$

Output
- SET gate, $p =$ given value

Typically, implication $v_"in"=>A$ is represented by having $A=or.big v_"in"$

The prover proves satisfication given $v_A$ against goals $P={p}$, showing $v_P$ which is the truthiness of each $p in v_P$

Previously, and in common trust systems, there is a transitive trust thing modelled by decaying implication between nodes.

I reasoned that without SET gate $p in v_P$ will always have scores from $v_A$, which violates rule 4.

In this case the degree of separation isn't represented, aka. the depth of a node.

=== Keeping depth information

Represent each node as a map, $n=d -> t$ rather than just $n=t$

$
  forall n_(n+1) "and" n => n_(n+1) = n "except when" d = n+1
$

Perfect implication doesn't exist in reality. In large depth of transitive modelling depth is the domiant factor.

Nodes in a web of trust are not propositions. They are people, and people make really bad judgements. The trust should decay exponentially.

Hardened, trust worthy gadgets are more trustworthy than votes by a million people, such as a battle-tested ZKP module, and a piece of hardware, and a political system that involves punishment which generates intrinsic _trust_. 

