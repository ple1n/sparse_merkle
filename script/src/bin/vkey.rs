use sp1_sdk::{include_elf, HashableKey, Prover, ProverClient};

pub const ELF_NAME: &[u8] = include_elf!("smt-program");

fn main() {
    let prover = ProverClient::builder().cpu().build();
    let (_, vk) = prover.setup(ELF_NAME);
    println!("{}", vk.bytes32());
}
