//! An end-to-end example of using the SP1 SDK to generate a proof of a program that can be executed
//! or have a core proof generated.
//!
//! You can run this script using the following command:
//! ```shell
//! RUST_LOG=info cargo run --release -- --execute
//! ```
//! or
//! ```shell
//! RUST_LOG=info cargo run --release -- --prove
//! ```

use std::{collections::BTreeMap, time::Instant};

use alloy_sol_types::SolType;
use clap::Parser;
use sha3::{digest::Update, Digest, Sha3_256};
use smt::smt::{FieldHasher, PartialTree, SparseMerkleTree};
use sp1_sdk::{include_elf, HashableKey, ProverClient, SP1Stdin};

/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
pub const ELF_NAME: &[u8] = include_elf!("smt-program");

/// The arguments for the command.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    execute: bool,

    #[arg(long)]
    prove: bool,

    #[arg(long, default_value = "20")]
    n: u32,
}

struct Sha3;

impl<const N: usize> FieldHasher<[u8; 32], N> for Sha3 {
    fn hash(&self, nodes: [[u8; 32]; N]) -> anyhow::Result<[u8; 32]> {
        let mut h = Sha3_256::new();
        for n in nodes {
            Update::update(&mut h, &n);
        }
        let f = h.finalize().to_vec();
        let mut s32 = [0; 32];
        s32.copy_from_slice(&f);
        Ok(s32)
    }
}

fn main() -> anyhow::Result<()> {
    // Setup the logger.
    sp1_sdk::utils::setup_logger();
    dotenv::dotenv().ok();

    // Parse the command line arguments.
    let args = Args::parse();

    if args.execute == args.prove {
        eprintln!("Error: You must specify either --execute or --prove");
        std::process::exit(1);
    }

    // Setup the prover client.
    let client = ProverClient::from_env();

    // Setup the inputs.
    let num_proofs = 5;
    let mut stdin = SP1Stdin::new();
    let mut tree_map: BTreeMap<u32, [u8; 32]> = BTreeMap::new();
    for n in 0..num_proofs {
        let mut val = [0; 32];
        val[2] = n as u8;
        if n % 2 == 0 {
            tree_map.insert(n, val);
        } else {
            tree_map.remove(&n);
        }
    }

    let h = Sha3;
    let tree: SparseMerkleTree<[u8; 32], Sha3, 32> = SparseMerkleTree::new(&tree_map, &h, [0; 32])?;

    let leaves: Vec<u64> = (0..num_proofs as u64).collect();
    let proof: PartialTree<[u8; 32], 32> = tree.batch_prove(&leaves);
    proof.verify(&Sha3).unwrap();

    stdin.write(&proof);

    if args.execute {
        // Execute the program
        let (output, report) = client.execute(ELF_NAME, &stdin).run().unwrap();
        println!("Program executed successfully.");

        println!("Number of cycles: {}", report.total_instruction_count());
    } else {
        // Setup the program for proving.
        let (pk, vk) = client.setup(ELF_NAME);

        println!("Proving {}", num_proofs);
        let tx = Instant::now();
        // Generate the proof
        let mut proof = client
            .prove(&pk, &stdin)
            .run()
            .expect("failed to generate proof");
        let d = Instant::now() - tx;
        let t = (d) / num_proofs;
        println!("Successfully generated proof! {:?}, {:?}", d, t);

        // let comm = proof.public_values.read::<[u8; 32]>();
        // dbg!(&comm);
        // let comm = proof.public_values.read::<Vec<u64>>();
        // dbg!(&comm);

        // Verify the proof.
        let verify_start = Instant::now();
        client.verify(&proof, &vk).expect("failed to verify proof");
        let verify_duration = verify_start.elapsed();
        println!("Proof verification took: {:?}", verify_duration);
    }

    Ok(())
}
