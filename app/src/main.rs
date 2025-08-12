use std::{collections::HashMap, env::current_dir, path::PathBuf};

use ark_circom::{CircomBuilder, CircomConfig};
use circom_prover::{
    prover::ProofLib,
    witness::{self, WitnessFn},
    CircomProver,
};
use circom_scotia::reader::load_r1cs;
use clap::Parser;
use freenet_ping_types::{Ping, PingContractOptions};
use freenet_stdlib::{
    client_api::{ClientRequest, ContractRequest},
    prelude::*,
};

mod ping_client;
use ping_client::{
    connect_to_host, run_ping_client, wait_for_get_response, wait_for_put_response,
    wait_for_subscribe_response,
};
use tracing::warn;

use bellman::{
    gadgets::{
        boolean::{AllocatedBit, Boolean},
        multipack,
        sha256::sha256,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use ff::PrimeField;
use pairing::Engine;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

#[derive(clap::Parser)]
#[command(author, version, about)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Run the bellpepper/circom-scotia test
    Nova,
    /// Run the default ping client
    Default(DefaultArgs),
    Groth16,
}

#[derive(clap::Args)]
struct DefaultArgs {
    #[clap(long, default_value = "localhost:50509")]
    host: String,
    #[clap(long, default_value = "info")]
    log_level: tracing::level_filters::LevelFilter,
    #[clap(flatten)]
    parameters: PingContractOptions,
    #[clap(long)]
    put_contract: bool,
    #[clap(long)]
    node_id: String,
}

const PACKAGE_DIR: &str = env!("CARGO_MANIFEST_DIR");
const PATH_TO_CONTRACT: &str = "../contracts/ping/build/freenet/freenet_ping_contract";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let args = Args::parse();

    // Determine log_level for tracing
    let log_level = match &args.command {
        Some(Command::Default(d)) => d.log_level,
        _ => tracing::level_filters::LevelFilter::INFO,
    };
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_level(true)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_max_level(log_level)
        .with_line_number(true)
        .init();

    match &args.command {
        Some(Command::Groth16) => {
            use ark_bn254::{Bn254, Fr};
            use ark_crypto_primitives::snark::SNARK;
            use ark_groth16::Groth16;
            use ark_std::rand::thread_rng;

            let base_path = "../circom/sha2".to_owned();
            let cfg = CircomConfig::<Fr>::new(
                base_path.clone() + "/sha2_js/sha2.wasm",
                base_path + "/sha2.r1cs",
            )?;
            let mut builder = CircomBuilder::new(cfg);

            builder.setup();

            builder.push_input("arg_in", 1);
            builder.push_input("arg_in", 2);

            // To be continued
            // From this point the groth16 verfiier from Arkworks can be used
            // But it only works with Bn254 which only has 100 bits of security.

            Ok(())
        }
        Some(Command::Nova) => {
            use std::{
                collections::HashMap,
                env::current_dir,
                io::Write,
                time::{Duration, Instant},
            };

            use ff::PrimeField;
            use ff::Field;
            use nova_scotia::{
                circom::reader::load_r1cs, create_public_params, create_recursive_circuit,
                FileLocation, F,
            };
            use nova_snark::traits::Group;
            use serde::{Deserialize, Serialize};
            use serde_json::json;

            type G1 = pasta_curves::pallas::Point;
            type G2 = pasta_curves::vesta::Point;

            let root = current_dir().unwrap();

            let circuit_file = FileLocation::PathBuf(root.join("../circom/sha2/sha2.r1cs"));
            let wn_file = FileLocation::PathBuf(root.join("../circom/sha2/sha2_js/sha2.wasm"));
            let r1cs = load_r1cs::<G1, G2>(&circuit_file);
            dbg!(r1cs.num_inputs);
            
            let mut pub_in = vec![];
            // pub_in.push(Field::ONE);
            // pub_in.push(Field::ONE);
            let mut priv_in = Vec::new();
            let mut input1 = HashMap::new();
            input1.insert("arg_in".to_owned(), json!([F::<G1>::from(0), F::<G1>::from(1)]));
            priv_in.push(input1);

            let pp = create_public_params::<G1, G2>(r1cs.clone());
            
            let circ = create_recursive_circuit(wn_file, r1cs, priv_in, pub_in, &pp)?;

            return Ok(());
        }
        _ => {
            // Default command (ping client)
            let default_args = match args.command {
                Some(Command::Default(d)) => d,
                _ => panic!("Default command args missing"),
            };

            // Connect to host using our utility function
            let mut client = connect_to_host(&default_args.host).await?;

            let params = Parameters::from(serde_json::to_vec(&default_args.parameters).unwrap());
            let path_to_code = PathBuf::from(PACKAGE_DIR).join(PATH_TO_CONTRACT);
            tracing::info!(path=%path_to_code.display(), "loading contract code");
            let code = std::fs::read(path_to_code).ok();

            let container = code
                .map(|bytes| ContractContainer::try_from((bytes, &params)))
                .transpose()?;
            let contract_key =
                ContractKey::from_params(default_args.parameters.code_key.clone(), params.clone())?;

            // Step 1: Put the contract or get it, and wait for response
            let mut local_state: Ping;

            if default_args.put_contract {
                // Put contract and wait for response
                let ping = Ping::default();
                let serialized = serde_json::to_vec(&ping)?;
                client
                    .send(ClientRequest::ContractOp(ContractRequest::Put {
                        contract: container.ok_or("contract not found while putting")?,
                        state: WrappedState::new(serialized),
                        related_contracts: RelatedContracts::new(),
                        subscribe: false,
                    }))
                    .await?;

                // Wait for put response
                let key = wait_for_put_response(&mut client, &contract_key).await?;
                tracing::info!(key=%key, "put ping contract successfully!");
                local_state = Ping::default();
            } else {
                // Get contract and wait for response
                client
                    .send(ClientRequest::ContractOp(ContractRequest::Get {
                        key: contract_key,
                        return_contract_code: true,
                        subscribe: false,
                    }))
                    .await?;

                // Wait for get response
                local_state = wait_for_get_response(&mut client, &contract_key).await?;
            }

            // Step 2: Subscribe to the contract and wait for subscription confirmation
            tracing::info!("Subscribing to contract...");
            client
                .send(ClientRequest::ContractOp(ContractRequest::Subscribe {
                    key: contract_key,
                    summary: None,
                }))
                .await?;

            // Wait for subscription response
            wait_for_subscribe_response(&mut client, &contract_key).await?;
            tracing::info!(key=%contract_key, "subscribed successfully!");

            // Run the main ping client logic

            run_ping_client(
                &mut client,
                contract_key,
                default_args.parameters,
                default_args.node_id.clone(),
                &mut local_state,
                None,
                None,
            )
            .await?;

            Ok(())
        }
    }
}
