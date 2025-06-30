#![no_main]

use sha3_v0_10_8::{digest::Update, Digest, Sha3_256};
use smt::smt::{FieldHasher, PartialTree, Path, Proof};

sp1_zkvm::entrypoint!(main);

pub fn main() {
    type F = [u8; 32];

    let path: PartialTree<F, 32> = sp1_zkvm::io::read();
    let h = Sha3_256::new();
    path.verify(&h).unwrap();
}
