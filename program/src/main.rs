//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]

use sha3_v0_10_8::{digest::Update, Digest, Sha3_256};
use smt::smt::{FieldHasher, Path, Proof};

sp1_zkvm::entrypoint!(main);

pub fn main() {
    let path: Proof<[u8; 32], 32> = sp1_zkvm::io::read();
    let h = Sha3_256::new();
    path.verify::<Sha3_256>(&h).unwrap();
}
