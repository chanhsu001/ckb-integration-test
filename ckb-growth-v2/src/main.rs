use crate::mining::mine;
use crate::node::Node;
use crate::utils::find_available_port;
use ckb_crypto::secp::{Privkey, Pubkey};
use ckb_growth::MAX_TXS_IN_NORMAL_MODE;
use ckb_hash::new_blake2b;
use ckb_system_scripts::BUNDLED_CELL;
use ckb_types::core::error::TransactionErrorSource::OutputsData;
use ckb_types::core::{BlockNumber, DepType, EpochNumberWithFraction};
use ckb_types::{
    bytes::Bytes,
    core::{
        capacity_bytes,
        cell::{resolve_transaction, OverlayCellProvider, TransactionsProvider},
        BlockBuilder, BlockView, Capacity, HeaderView, ScriptHashType, TransactionBuilder,
        TransactionView,
    },
    h160, h256,
    packed::{
        Byte32, CellDep, CellInput, CellOutput, OutPoint, ProposalShortId, Script, WitnessArgs,
    },
    prelude::*,
    H160, H256, U256,
};
use clap::{Args, Parser, Subcommand};
use lazy_static::lazy_static;
use std::env;
use std::path::PathBuf;
use std::process::exit;

mod mining;
mod node;
mod rpc;
mod utils;

#[derive(Parser)]
#[clap(
    name = "ckb_growth",
    author = "Nervos Core Dev <dev@nervos.org>",
    about = "Nervos CKB - The Common Knowledge Base"
)]
/// command line structure for clap parsed
pub struct CkbGrowth {
    /// ckb subcommand
    #[clap(subcommand)]
    pub sub_command: GrowthSubCommand,
}

#[derive(Subcommand)]
#[clap()]
/// ckb subcommand
pub enum GrowthSubCommand {
    /// miner subcommand
    #[clap(about = "Runs ckb miner")]
    Run(CmdRun),
}

#[derive(Args)]
#[clap()]
pub struct CmdRun {
    #[clap(short, long, default_value = "./data")]
    /// Data directory
    data_dir: PathBuf,

    // #[clap(short, long)]
    // /// How long it takes to mine a block
    // tx_interval_ms: u64,
    #[clap(short, long)]
    /// normal mode data expansion in 5 year
    normal_expansion: bool,

    #[clap(short, long)]
    /// maximum mode data expansion in 1 year
    maximum_expansion: bool,
}

lazy_static! {
    static ref SECP_DATA_CELL: (CellOutput, Bytes) = {
        let raw_data = BUNDLED_CELL
            .get("specs/cells/secp256k1_data")
            .expect("load secp256k1_data");
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

/// get lock_args of the account public key
fn get_lock_args(public_key: &Pubkey) -> Script {
    let (_, data) = secp_data_cell();
    Script::new_builder()
        .code_hash(CellOutput::calc_data_hash(&data))
        .args(Bytes::from(public_key.serialize()).pack())
        .hash_type(ScriptHashType::Data.into())
        .build()
}

pub fn secp_cell() -> &'static (CellOutput, Bytes) {
    &SECP_CELL
}

pub fn secp_data_cell() -> &'static (CellOutput, Bytes) {
    &SECP_DATA_CELL
}

/// representation of every account
#[derive(Clone)]
pub struct Account {
    private_key: Privkey,
    lock_args: Script,
    cell_cap: u64,
}

impl Account {
    pub fn new(private_key: H256, cell_cap: u64) -> Self {
        let private_key = Privkey::from(private_key);
        let public_key = private_key.pubkey().expect("pubkey() error?");
        let lock_args = get_lock_args(&public_key);

        Self {
            private_key,
            lock_args,
            cell_cap,
        }
    }

    /// create a new account, derived from owner private key
    pub fn derive_new_account(&self) -> Account {
        let template = self.private_key.pubkey().expect("pubkey").serialize();
        let next_privkey = ckb_hash::blake2b_256(template).pack();
        let private_key: Privkey = Privkey::from_slice(next_privkey.as_slice());
        let public_key = private_key.pubkey().expect("pubkey() failed");

        Account {
            private_key,
            lock_args: get_lock_args(&public_key),
            cell_cap: 0,
        }
    }
}

const MIN_FEE_RATE: u64 = 1000;
const MIN_CELL_CAP: u64 = 69_00_000_0000;

type MillionHeight = u64;
type LiveCellCnt = u64;
type TxCnt = u64;
/// normal mode, each million height, one block contains how many live_cells and 2in2out tx
static NORMAL_PHASE_CELLS_TXS_CNT: [(MillionHeight, LiveCellCnt, TxCnt); 15] = [
    (1, 1, 1),
    (2, 1, 1),
    (3, 1, 1),
    (4, 2, 2),
    (5, 2, 2),
    (6, 3, 2),
    (7, 3, 3),
    (8, 4, 4),
    (9, 4, 5),
    (10, 5, 5),
    (11, 5, 6),
    (12, 6, 7),
    (13, 6, 7),
    (14, 7, 8),
    (15, 7, MAX_TXS_IN_NORMAL_MODE),
];

/// maximum mode, each million height, one block contains how many live_cells and 2in2out tx
static MAX_PHASE_CELLS_TXS_CNT: [(MillionHeight, LiveCellCnt, TxCnt); 7] = [
    (1, 1, 1),
    (2, 2, 1),
    (3, 1, 1),
    (4, 2, 2),
    (5, 2, 1),
    (6, 1, 1),
    (7, 1, 1),
];

/// return each block should contains livecells count and transfer-txs count at specific height
fn get_livecellcnt_txcnt(height: u64) -> (LiveCellCnt, TxCnt) {
    for (million_height, livecell_cnt, txs_cnt) in NORMAL_PHASE_CELLS_TXS_CNT.iter() {
        if height < million_height * 1000000 {
            return (*livecell_cnt, *txs_cnt);
        }
    }
    panic!("not possible to mis-match!");
}

pub fn secp256k1_cell_dep(genesis_block: &BlockView) -> Vec<CellDep> {
    use ckb_types::prelude::Pack;
    let mut v = vec![];
    let op = OutPoint::new_builder()
        .tx_hash(
            genesis_block
                .transaction(0)
                .expect("index genesis dep-group transaction")
                .hash(),
        )
        .index(0_usize.pack())
        .build();
    v.push(
        CellDep::new_builder()
            .out_point(op)
            .dep_type(DepType::DepGroup.into())
            .build(),
    );

    let op = OutPoint::new_builder()
        .tx_hash(
            genesis_block
                .transaction(0)
                .expect("index genesis dep-group transaction")
                .hash(),
        )
        .index(1_usize.pack())
        .build();
    v.push(
        CellDep::new_builder()
            .out_point(op)
            .dep_type(DepType::DepGroup.into())
            .build(),
    );
    v
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

/// build 1in-Nout transaction to create N output_cell out of 1 input_cell on one account
/// the 1st cell capacity is nearly equal to input cell, the other cells capacity is tiny
pub fn gen_live_cells(parent: &BlockView, account: &Account, livecell_cnt: u64, secp_cell_deps: &Vec<CellDep>) -> TransactionView {
    let input = {
        let txs = parent.transactions();

        // if parent block is genesis, input cell is at tx_0 and len-1 index
        if parent.header().number() == 0 {
            println!("genesis block input cells");
            // the 2nd tx in parent block is input cell for this tx
            let tx = txs.get(0).expect("get 1st live_cell transaction");
            println!(
                "genesis tx_hash:{}, outputs number:{}",
                tx.hash(),
                tx.outputs().len()
            );
            // CellInput::new(OutPoint::new(tx.hash(), (tx.outputs().len() - 1) as u32), 0)
            CellInput::new(OutPoint::new(tx.hash(), 7), 0)
        } else {
            // the 2nd tx in parent block is input cell for this tx
            let tx = txs.get(1).expect("get live_cell transaction");
            CellInput::new(OutPoint::new(tx.hash(), 0), parent.header().number())
        }
    };

    let origin_cap = Capacity::zero()
        .safe_add(account.cell_cap)
        .expect("origin capacity");
    let rest = origin_cap.safe_sub(MIN_FEE_RATE as u64).expect("for min_fee_rate");
    let rest = rest
        .safe_sub(MIN_CELL_CAP * livecell_cnt)
        .expect("sub live cells capacity");

    let mut outputs = vec![];
    outputs.push(
        CellOutput::new_builder()
            .capacity(rest.as_u64().pack())
            .lock(account.lock_args.clone())
            .build(),
    );
    (0..livecell_cnt).for_each(|_| {
        outputs.push(
            CellOutput::new_builder()
                .capacity(MIN_CELL_CAP.pack())
                .lock(account.lock_args.clone())
                .build()
        );
    });

    let mut outputs_data = vec![];
    (0..=livecell_cnt).for_each(|i| {
        outputs_data.push(Bytes::from(i.to_le_bytes().to_vec()));
    });

    let tx = TransactionBuilder::default()
        .input(input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_deps(secp_cell_deps.clone())
        .build();
    let accounts = [account.clone()];
    attach_witness(tx, &accounts)
}

fn main() {
    let _logger = init_logger();
    let cli = CkbGrowth::parse();

    return match &cli.sub_command {
        GrowthSubCommand::Run(matches) => cmd_run(matches),
    };
}

fn cmd_run(matches: &CmdRun) {
    let data_dir = &matches.data_dir;
    let normal_mode = matches.normal_expansion;
    let maximum_mode = matches.maximum_expansion;

    if normal_mode == false && maximum_mode == false {
        eprintln!("must specific expansion mode: normal or maximum");
        exit(-1);
    }
    if normal_mode == true {
        println!("normal mode in 5 years data expansion");
        normal_expansion(data_dir);
    } else {
        println!("maximum mode in 1 years data expansion");
        //maximum_expansion(data_dir, t_tx_interval);
    }
}

fn normal_expansion(data_dir: &PathBuf) {
    // let mut node = Node::new(data_dir.clone());
    let mut node = Node::new(PathBuf::from("./"));

    // modify app config
    // node.modify_app_config(|app_config| {
    //     let rpc_port = find_available_port();
    //     let p2p_port = find_available_port();
    //     app_config.rpc.listen_address = format!("127.0.0.1:{}", rpc_port);
    //     app_config.network.listen_addresses =
    //         vec![format!("/ip4/127.0.0.1/tcp/{}", p2p_port).parse().unwrap()];
    // });

    // modify chain spec
    node.modify_chain_spec(|spec| {});

    let genesis_block = node.get_tip_block();
    let cell_dep = secp256k1_cell_dep(&genesis_block);
    mine(&node, 19);

    // the account embedded accounts in Dev chain
    // account for live cells generation
    let cellbase_account = Account::new(
        h256!("0xd00c06bfd800d27397002dca6fb0993d5ba6399b4238b2f29ee9deb97593d2bc"),
        20_000_000_000_00000000,
    );

    // base account, derive more accounts for building 2in2out tx
    let owner_account =
        // primary cellbase account
        Account::new(
            h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
            5_198_735_037_00000000,
        );

    // #20 block
    {
        let height = 20;
        let (livecell_cnt, txs_cnt) = get_livecellcnt_txcnt(height);

        // prepare live cell input
        let mut block = node.new_block(None, None, None);
        let live_cells_tx = gen_live_cells(&genesis_block, &cellbase_account, livecell_cnt, &cell_dep);
        node.submit_transaction(&live_cells_tx);

        // Todo: prepare 4 accounts and 2in2out input cells

        let builder = block
            .as_advanced_builder()
            .transactions(vec![live_cells_tx]);

        // node.submit_block(&builder.build());

        //disable verify, submit block
        node.process_block_without_verify(&builder.build(), false);
    }

    let height = 21;
    let (livecell_cnt, txs_cnt) = get_livecellcnt_txcnt(height);

    // #21 block
    {
        // we assume the live cell input is ready at #20 block, split the input cell and make more live cells
        let parent = node.get_tip_block();
        let mut block = node.new_block(None, None, None);
        let live_cells_tx = gen_live_cells(&parent, &cellbase_account, livecell_cnt, &cell_dep);
        let builder = block
            .as_advanced_builder()
            .transactions(vec![live_cells_tx]);

        //disable verify, submit block
        node.process_block_without_verify(&builder.build(), false);
    }
}

fn init_logger() -> ckb_logger_service::LoggerInitGuard {
    let filter = match env::var("RUST_LOG") {
        Ok(filter) if filter.is_empty() => Some("info".to_string()),
        Ok(filter) => Some(filter.to_string()),
        Err(_) => Some("info".to_string()),
    };
    let config = ckb_logger_config::Config {
        filter,
        color: false,
        log_to_file: false,
        log_to_stdout: true,
        ..Default::default()
    };
    ckb_logger_service::init(None, config)
        .unwrap_or_else(|err| panic!("failed to init the logger service, error: {}", err))
}
