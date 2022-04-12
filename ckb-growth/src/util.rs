use ckb_app_config::{DBConfig, NetworkConfig};
use ckb_async_runtime::Handle;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use ckb_chain::chain::{ChainController, ChainService};
use ckb_chain_spec::ChainSpec;
use ckb_chain_spec::consensus::{ConsensusBuilder, ProposalWindow};
use ckb_crypto::secp::{Privkey, Pubkey};
use ckb_dao::DaoCalculator;
use ckb_dao_utils::genesis_dao_data;
use ckb_hash::new_blake2b;
use ckb_launcher::SharedBuilder;
use ckb_network::{DefaultExitHandler, NetworkController, NetworkService, NetworkState};
use ckb_resource::Resource;
use ckb_shared::{Shared, Snapshot};
use ckb_store::ChainStore;
use ckb_system_scripts::BUNDLED_CELL;
use ckb_types::core::{BlockNumber, EpochNumberWithFraction};
use ckb_types::{
    bytes::Bytes,
    core::{
        capacity_bytes,
        cell::{resolve_transaction, OverlayCellProvider, TransactionsProvider},
        BlockBuilder, BlockView, Capacity, HeaderView, ScriptHashType, TransactionBuilder,
        TransactionView,
    },
    packed::{
        Byte32, CellDep, CellInput, CellOutput, OutPoint, ProposalShortId, Script, WitnessArgs,
    },
    prelude::*,
    H160, H256, U256
};
use ckb_types::utilities::difficulty_to_compact;

use crate::MAX_TXS_IN_NORMAL_MODE;
use lazy_static::lazy_static;
use rand::random;

pub struct Chain {
    pub controller: ChainController,
    pub shared: Shared,
}
impl Chain {
    pub fn new(controller: ChainController, shared: Shared) -> Self {
        Chain { controller, shared }
    }
}

// /// every live cells representation
// struct LiveCell {
//     tx_hash: Byte32,
//     // output index in the tx
//     index: u32,
//     // the block number tx belongs to
//     block_number: u64,
//     // capacity
//     capacity: u64,
// }

/// representation of every account
#[derive(Clone)]
pub struct Account {
    private_key: H256,
    lock_args: Script,
    cell_cap: u64,
}

impl Account {
    pub fn new(private_key: H256, public_key: H160, cell_cap: u64) -> Self {
        Self {
            private_key,
            lock_args: get_lock_args(&public_key),
            cell_cap,
        }
    }

    /// create a new account, derived from owner private key
    pub fn derive_new_account(&self) -> Account {
        let template_privkey = self.private_key.as_bytes();
        let next_privkey = ckb_hash::blake2b_256(template_privkey).pack();
        let private_key: Privkey = Privkey::from_slice(next_privkey.as_slice());
        let public_key = private_key.pubkey().expect("pubkey() failed");
        let lock_args = H160::from_slice(ckb_hash::blake2b_256(public_key.as_bytes()).as_slice())
            .expect("H160::from_slice() failed");

        Account {
            private_key: H256::from_slice(next_privkey.as_slice()).expect("H256 from_slice"),
            lock_args: get_lock_args(&lock_args),
            cell_cap: 0,
        }
    }
}

const GENESIS_LIVECELL_CAP: usize = 20_000_000_000_00000000;

lazy_static! {
    static ref SECP_DATA_CELL: (CellOutput, Bytes) = {
        let raw_data = BUNDLED_CELL
            .get("specs/cells/secp256k1_data")
            .expect("load secp256k1_blake160_sighash_all");
        let data: Bytes = raw_data.to_vec().into();

        let cell = CellOutput::new_builder()
            .capacity(Capacity::bytes(data.len()).unwrap().pack())
            .build();
        (cell, data)
    };
    static ref SECP_CELL: (CellOutput, Bytes) = {
        let raw_data = BUNDLED_CELL
            .get("specs/cells/secp256k1_blake160_sighash_all")
            .expect("load secp256k1_blake160_sighash_all");
        let data: Bytes = raw_data.to_vec().into();

        let cell = CellOutput::new_builder()
            .capacity(Capacity::bytes(data.len()).unwrap().pack())
            .build();
        (cell, data)
    };
}

/// get lock_args of the account public key hash
fn get_lock_args(public_key_hash: &H160) -> Script {
    let (_, data) = secp_data_cell();
    Script::new_builder()
        .code_hash(CellOutput::calc_data_hash(&data))
        .args(Bytes::copy_from_slice(public_key_hash.as_bytes()).pack())
        .hash_type(ScriptHashType::Data.into())
        .build()
}

pub fn secp_cell() -> &'static (CellOutput, Bytes) {
    &SECP_CELL
}

pub fn secp_data_cell() -> &'static (CellOutput, Bytes) {
    &SECP_DATA_CELL
}

// // deploy secp smart-contract transaction on chain
// pub fn create_secp_tx(account: &Account) -> TransactionView {
//     let (ref secp_data_cell, ref secp_data_cell_data) = secp_data_cell();
//     let (ref secp_cell, ref secp_cell_data) = secp_cell();
//     let outputs = vec![secp_data_cell.clone(), secp_cell.clone()];
//     let outputs_data = vec![secp_data_cell_data.pack(), secp_cell_data.pack()];
//     TransactionBuilder::default()
//         .witness(account.lock_args.clone().into_witness())
//         .input(CellInput::new(OutPoint::null(), 0))
//         .outputs(outputs)
//         .outputs_data(outputs_data)
//         .build()
// }

/// build 1in-Nout transaction to create N output_cell out of 1 input_cell on one account
/// the 1st cell capacity is nearly equal to input cell, the other cells capacity is tiny
pub fn gen_live_cells(
    parent: &BlockView,
    account: &Account,
    current_height: BlockNumber,
    livecell_cnt: u64,
) -> TransactionView {
    let input = {
        let txs = parent.transactions();
        // the 2nd tx in parent block is input cell for this tx
        let tx = txs.get(1).expect("get 1st live_cell transaction");
        CellInput::new(OutPoint::new(tx.hash(), 0), current_height - 1)
    };

    let most_capacity = Capacity::bytes(GENESIS_LIVECELL_CAP - (80 * livecell_cnt) as usize)
        .expect("capacity overflow?");
    let most_output = CellOutput::new_builder()
        .capacity(most_capacity.pack())
        .lock(account.lock_args.clone())
        .build();

    let tiny_outputs: Vec<CellOutput> = (0..livecell_cnt)
        .map(|_| {
            CellOutput::new_builder()
                .capacity(80.pack())
                .lock(account.lock_args.clone())
                .build()
        })
        .collect();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(most_output)
        .outputs(tiny_outputs)
        .build();
    let accounts = [account.clone()];
    attach_witness(tx, &accounts)
}

/// attach witness to unsigned tx
fn attach_witness(mut tx: TransactionView, signed_accounts: &[Account]) -> TransactionView {
    // handle signature
    let witness = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    let witness_len = witness.as_slice().len() as u64;
    let message = {
        let mut hasher = new_blake2b();
        hasher.update(&*tx.hash().as_bytes());
        hasher.update(&witness_len.to_le_bytes());
        hasher.update(witness.as_slice());
        for _ in 1..signed_accounts.len() {
            let more_witness = Bytes::new();
            let more_witness_len = more_witness.len() as u64;
            hasher.update(&more_witness_len.to_le_bytes());
            hasher.update(&more_witness);
        }
        let mut buf = [0u8; 32];
        hasher.finalize(&mut buf);
        H256::from(buf)
    };

    for account in signed_accounts {
        let witness = WitnessArgs::new_builder()
            .lock(Some(Bytes::from(vec![0u8; 65])).pack())
            .build();
        let sig = Privkey::from(account.private_key.clone())
            .sign_recoverable(&message)
            .expect("sign");
        let w = witness
            .as_builder()
            .lock(Some(Bytes::from(sig.serialize())).pack())
            .build();
        tx = tx
            .as_advanced_builder()
            .witness(w.as_bytes().pack())
            .build();
    }

    tx
}

/// new dev chain(pow = dummy), secp256k1, and enough cell for test account in genesis block
pub fn new_secp_dev_chain(data_path: &PathBuf, handle: Handle) -> Chain {
    let spec =
        ChainSpec::load_from(&Resource::file_system(PathBuf::from("specs/dev.toml"))).unwrap();
    let consensus = spec.build_consensus().unwrap();

    let mut db_config = DBConfig::default();
    db_config.path = PathBuf::from("./data/db");

    let shared_builder =
        SharedBuilder::new("ckb-growth", data_path, &db_config, None, handle).unwrap();
    let (shared, mut pack) = shared_builder.consensus(consensus.clone()).build().unwrap();

    let network = dummy_network(&shared);
    pack.take_tx_pool_builder().start(network);

    let chain_service = ChainService::new(shared.clone(), pack.take_proposal_table());

    Chain::new(chain_service.start::<&str>(None), shared)
}


pub fn create_secp_tx(account: &Account) -> TransactionView {
    let (ref secp_data_cell, ref secp_data_cell_data) = secp_data_cell();
    let (ref secp_cell, ref secp_cell_data) = secp_cell();
    let script = account.lock_args.clone();
    let outputs = vec![secp_data_cell.clone(), secp_cell.clone()];
    let outputs_data = vec![secp_data_cell_data.pack(), secp_cell_data.pack()];
    TransactionBuilder::default()
        .witness(script.clone().into_witness())
        .input(CellInput::new(OutPoint::null(), 0))
        .outputs(outputs)
        .outputs_data(outputs_data)
        .build()
}

pub fn new_secp_dev_chain_raw(owner_account: &Account) -> Chain {
    // let secp_script = owner_account.lock_args.clone();
    let tx = create_secp_tx(owner_account);
    let dao = genesis_dao_data(vec![&tx]).unwrap();

    // create genesis block with N txs
    // let transactions: Vec<TransactionView> = (0..txs_size)
    //     .map(|i| {
    //         let data = Bytes::from(i.to_le_bytes().to_vec());
    //         let output = CellOutput::new_builder()
    //             .capacity(capacity_bytes!(50_000).pack())
    //             .lock(secp_script.clone())
    //             .build();
    //         TransactionBuilder::default()
    //             .input(CellInput::new(OutPoint::null(), 0))
    //             .output(output.clone())
    //             .output(output)
    //             .output_data(data.pack())
    //             .output_data(data.pack())
    //             .build()
    //     })
    //     .collect();

    let genesis_block = BlockBuilder::default()
        .compact_target(difficulty_to_compact(U256::from(1000u64)).pack())
        .dao(dao)
        .transaction(tx)
        // .transactions(transactions)
        .build();

    let mut consensus = ConsensusBuilder::default()
        .cellbase_maturity(EpochNumberWithFraction::new(0, 0, 1))
        .genesis_block(genesis_block)
        .build();
    consensus.tx_proposal_window = ProposalWindow(1, 10);

    let (shared, mut pack) = SharedBuilder::with_temp_db()
        .consensus(consensus.clone())
        .build()
        .unwrap();
    let chain_service = ChainService::new(shared.clone(), pack.take_proposal_table());

    Chain {
        controller: chain_service.start::<&str>(None),
        shared
    }
}


/// build a secp cellbase tx with account
pub fn create_secp_cellbase(
    shared: &Shared,
    parent: &HeaderView,
    account: &Account,
) -> TransactionView {
    let raw_data = BUNDLED_CELL
        .get("specs/cells/secp256k1_blake160_sighash_all")
        .expect("load secp256k1_blake160_sighash_all");
    let data: Bytes = raw_data.to_vec().into();

    let script = Script::new_builder()
        .code_hash(CellOutput::calc_data_hash(&data))
        .args(Bytes::from(account.lock_args.as_bytes()).pack())
        .hash_type(ScriptHashType::Data.into())
        .build();

    let capacity = calculate_reward(shared, parent);

    let builder = TransactionBuilder::default()
        .input(CellInput::new_cellbase_input(parent.number() + 1))
        .witness(script.into_witness());

    if (parent.number() + 1) <= shared.consensus().finalization_delay_length() {
        builder.build()
    } else {
        builder
            .output(
                CellOutput::new_builder()
                    .capacity(capacity.pack())
                    .lock(account.lock_args.clone())
                    .build(),
            )
            .output_data(Bytes::new().pack())
            .build()
    }
}

/// build block with user defined transaction, the block embedded with one secp cellbase tx
pub fn gen_secp_block(
    p_block: &BlockView,
    shared: &Shared,
    account: &Account,
    if_cellbase: bool,
    txs_proposal: Vec<ProposalShortId>,
    txs_except_cellbase: Vec<TransactionView>,
) -> BlockView {
    let (number, timestamp) = (
        p_block.header().number() + 1,
        p_block.header().timestamp() + 10000,
    );

    let mut txs_to_resolve = vec![];
    let cellbase = create_secp_cellbase(shared, &p_block.header(), account);

    if if_cellbase {
        txs_to_resolve.push(cellbase.clone());
    }
    txs_to_resolve.extend_from_slice(&txs_except_cellbase);
    let dao = dao_data(shared, &p_block.header(), &txs_to_resolve);

    let epoch = shared
        .consensus()
        .next_epoch_ext(&p_block.header(), &shared.store().as_data_provider())
        .unwrap()
        .epoch();

    let mut block = BlockBuilder::default().build();
    if if_cellbase {
        block = block.as_advanced_builder().transaction(cellbase).build();
    }
    block
        .as_advanced_builder()
        .transactions(txs_except_cellbase)
        .proposals(txs_proposal)
        .parent_hash(p_block.hash())
        .number(number.pack())
        .timestamp(timestamp.pack())
        .compact_target(epoch.compact_target().pack())
        .epoch(epoch.number_with_fraction(number).pack())
        .nonce(random::<u128>().pack())
        .dao(dao)
        .build();

    block
}

fn create_transaction(parent_hash: &Byte32, lock: Script, dep: OutPoint) -> TransactionView {
    let data: Bytes = (0..255).collect();
    TransactionBuilder::default()
        .output(
            CellOutput::new_builder()
                .capacity(capacity_bytes!(50_000).pack())
                .lock(lock)
                .build(),
        )
        .output_data(data.pack())
        .input(CellInput::new(OutPoint::new(parent_hash.to_owned(), 0), 0))
        .cell_dep(CellDep::new_builder().out_point(dep).build())
        .build()
}

/// create tx to generate enough cells for all 2in2out tx in normal expansion mode
/// input cells is block#1-17's output cells
/// should call it at block #18
pub fn create_input_cells_in_normal_mode(
    account: &Account,
    previous_cellbase_info: &Vec<(Byte32, BlockNumber)>,
    secp_cell_deps: &Vec<CellDep>,
) -> TransactionView {
    let inputs: Vec<CellInput> = previous_cellbase_info
        .iter()
        .take(2)
        .map(|(hash, block_number)| CellInput::new(OutPoint::new(hash.clone(), 0), *block_number))
        .collect();

    let outputs: Vec<CellOutput> = (0..MAX_TXS_IN_NORMAL_MODE * 6)
        .map(|_| {
            CellOutput::new_builder()
                .capacity(80.pack())
                .lock(account.lock_args.clone())
                .build()
        })
        .collect();

    let tx = TransactionBuilder::default()
        .inputs(inputs)
        .outputs(outputs)
        .cell_deps(secp_cell_deps.clone())
        .build();

    // handle signature
    let accounts = [account.clone()];
    attach_witness(tx, &accounts)
}

/// create txs_count 2in2out secp transactions, locked by accounts.
/// input cells are all in parent block transactions[2]
pub fn create_2in2out_transactions(
    parent: &BlockView,
    input_accounts: &[Account],
    output_accounts: &[Account],
    txs_count: u32,
    cell_deps: &Vec<CellDep>,
) -> Vec<TransactionView> {
    let mut txs: Vec<TransactionView> = vec![];

    for tx_index in 0..txs_count {
        // input based on parent 3nd tx
        let cell_inputs: Vec<CellInput> = (0..2)
            .map(|i| {
                CellInput::new(
                    OutPoint::new(parent.transactions()[2].hash(), i + tx_index),
                    parent.header().number(),
                )
            })
            .collect();
        let cell_outputs: Vec<CellOutput> = (0..2)
            .zip(output_accounts.iter())
            .map(|(_, account)| {
                CellOutput::new_builder()
                    .capacity(capacity_bytes!(50_000).pack())
                    .lock(account.lock_args.clone())
                    .build()
            })
            .collect();

        let raw = TransactionBuilder::default()
            .outputs(cell_outputs)
            .inputs(cell_inputs)
            .cell_deps(cell_deps.clone())
            .build();

        let tx = attach_witness(raw, &input_accounts);
        txs.push(tx);
    }

    txs
}

pub fn dao_data(shared: &Shared, parent: &HeaderView, txs: &[TransactionView]) -> Byte32 {
    let mut seen_inputs = HashSet::new();
    // In case of resolving errors, we just output a dummp DAO field,
    // since those should be the cases where we are testing invalid
    // blocks
    let transactions_provider = TransactionsProvider::new(txs.iter());
    let snapshot: &Snapshot = &shared.snapshot();
    let overlay_cell_provider = OverlayCellProvider::new(&transactions_provider, snapshot);
    let rtxs = txs.iter().cloned().try_fold(vec![], |mut rtxs, tx| {
        let rtx = resolve_transaction(tx, &mut seen_inputs, &overlay_cell_provider, snapshot);
        match rtx {
            Ok(rtx) => {
                rtxs.push(rtx);
                Ok(rtxs)
            }
            Err(e) => Err(e),
        }
    });
    let rtxs = rtxs.expect("dao_data resolve_transaction");
    let data_loader = snapshot.as_data_provider();
    let calculator = DaoCalculator::new(snapshot.consensus(), &data_loader);
    calculator
        .dao_field(&rtxs, parent)
        .expect("calculator dao_field")
}

pub(crate) fn calculate_reward(shared: &Shared, parent: &HeaderView) -> Capacity {
    let number = parent.number() + 1;
    let snapshot = shared.snapshot();
    let target_number = shared.consensus().finalize_target(number).unwrap();
    let target_hash = snapshot.get_block_hash(target_number).unwrap();
    let target = snapshot.get_block_header(&target_hash).unwrap();
    let data_loader = snapshot.as_data_provider();
    let calculator = DaoCalculator::new(shared.consensus(), &data_loader);
    calculator
        .primary_block_reward(&target)
        .expect("calculate_reward primary_block_reward")
        .safe_add(calculator.secondary_block_reward(&target).unwrap())
        .expect("calculate_reward safe_add")
}

fn dummy_network(shared: &Shared) -> NetworkController {
    let tmp_dir = tempfile::Builder::new().tempdir().unwrap();
    let config = NetworkConfig {
        max_peers: 19,
        max_outbound_peers: 5,
        path: tmp_dir.path().to_path_buf(),
        ping_interval_secs: 15,
        ping_timeout_secs: 20,
        connect_outbound_interval_secs: 1,
        discovery_local_address: true,
        bootnode_mode: true,
        reuse_port_on_linux: true,
        ..Default::default()
    };

    let network_state =
        Arc::new(NetworkState::from_config(config).expect("Init network state failed"));
    NetworkService::new(
        network_state,
        vec![],
        vec![],
        shared.consensus().identify_name(),
        "test".to_string(),
        DefaultExitHandler::default(),
    )
    .start(shared.async_handle())
    .expect("Start network service failed")
}
