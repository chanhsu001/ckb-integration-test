extern crate core;

use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::panic;
use std::path::PathBuf;
use std::process::exit;

use ckb_chain_spec::consensus::TYPE_ID_CODE_HASH;
use ckb_crypto::secp::Privkey;
use ckb_hash::{blake2b_256, new_blake2b};
use ckb_jsonrpc_types::CellWithStatus;
use ckb_logger::debug;
use ckb_system_scripts::BUNDLED_CELL;
use ckb_types::core::DepType;
use ckb_types::{
    bytes::Bytes,
    core::{BlockView, Capacity, ScriptHashType, TransactionBuilder, TransactionView},
    h256, packed,
    packed::{CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
    prelude::*,
    H256,
};
use clap::{Args, Parser, Subcommand};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use ckb_growth::MAX_TXS_IN_NORMAL_MODE;

use crate::mining::mine;
use crate::node::Node;

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
    #[clap(short, long)]
    /// normal mode data expansion in 5 year
    normal_expansion: bool,

    #[clap(short, long)]
    /// maximum mode data expansion in 1 year
    maximum_expansion: bool,

    #[clap(short, long, default_value_t = 0)]
    /// Specifies ckb growth start `from` block number
    from: u64,

    #[clap(short, long, default_value_t = 16_000_000)]
    /// Specifies ckb growth halt after commit the block of `to` number
    to: u64,
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

/// correct version: get lock_args of the account public key
fn get_lock_args_from_bytes(bytes: &Bytes) -> Script {
    Script::new_builder()
        .args(bytes.pack())
        .code_hash(type_lock_script_code_hash().pack())
        .hash_type(ScriptHashType::Type.into())
        .build()
}
/// The output index of SECP256K1/blake160 script in the genesis no.0 transaction
pub const OUTPUT_INDEX_SECP256K1_BLAKE160_SIGHASH_ALL: u64 = 1;

fn type_lock_script_code_hash() -> H256 {
    build_genesis_type_id_script(OUTPUT_INDEX_SECP256K1_BLAKE160_SIGHASH_ALL)
        .calc_script_hash()
        .unpack()
}
/// Shortcut for build genesis type_id script from specified output_index
pub fn build_genesis_type_id_script(output_index: u64) -> packed::Script {
    build_type_id_script(&packed::CellInput::new_cellbase_input(0), output_index)
}
pub(crate) fn build_type_id_script(input: &packed::CellInput, output_index: u64) -> packed::Script {
    let mut blake2b = new_blake2b();
    blake2b.update(input.as_slice());
    blake2b.update(&output_index.to_le_bytes());
    let mut ret = [0; 32];
    blake2b.finalize(&mut ret);
    let script_arg = Bytes::from(ret.to_vec());
    packed::Script::new_builder()
        .code_hash(TYPE_ID_CODE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(script_arg.pack())
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
    bytes_lock_args: Bytes,
    cell_cap: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountCellCap {
    cellbase_cap: u64,
    owner_cap: u64,
    owner_derived_cap: (u64, u64, u64),
}

impl Account {
    pub fn new(private_key: H256, cell_cap: u64) -> Self {
        let private_key = Privkey::from(private_key);
        let public_key = private_key.pubkey().expect("pubkey() error?");
        let bytes_lock_args = Bytes::from(blake2b_256(&public_key.serialize())[0..20].to_vec());
        let lock_args = get_lock_args_from_bytes(&bytes_lock_args);

        Self {
            private_key,
            lock_args,
            bytes_lock_args,
            cell_cap,
        }
    }

    /// create a new account, derived from owner private key
    pub fn derive_new_account(&self) -> Account {
        let template = self.private_key.pubkey().expect("pubkey").serialize();
        let next_privkey = ckb_hash::blake2b_256(template).pack();
        let private_key: Privkey = Privkey::from_slice(next_privkey.as_slice());
        let public_key = private_key.pubkey().expect("pubkey() failed");
        let bytes_lock_args = Bytes::from(blake2b_256(&public_key.serialize())[0..20].to_vec());
        let lock_args = get_lock_args_from_bytes(&bytes_lock_args);

        Account {
            private_key,
            lock_args,
            bytes_lock_args,
            cell_cap: 0,
        }
    }
}
impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        writeln!(
            f,
            "\npublic key :{}\nbytes_lock_args  :{:x}\nlock_args  :{:x}",
            self.private_key.pubkey().unwrap(),
            self.bytes_lock_args,
            self.lock_args
        )
    }
}

// const MIN_FEE_RATE: u64 = 1_000;
// disable FEE_RATE for simplification
const MIN_FEE_RATE: u64 = 0;
const MIN_CELL_CAP: u64 = 9_000_000_000;
const TWO_TWO_START_HEIGHT: u64 = 20;
const MILLION_HEIGHT: u64 = 1_000_000;

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
static MAX_PHASE_CELLS_TXS_CNT: [(MillionHeight, LiveCellCnt, TxCnt); 10] = [
    (1, 1, 1),
    (2, 2, 1),
    (3, 1, 1),
    (4, 2, 2),
    (5, 2, 2),
    (6, 3, 2),
    (7, 3, 3),
    (8, 4, 1000),
    (9, 4, 1000),
    (10, 5, 1000),
];

/// return each block should contains livecells count and transfer-txs count at specific height
fn get_livecellcnt_txcnt(mode: ExpansionMode, height: u64) -> (LiveCellCnt, TxCnt) {
    if mode == ExpansionMode::NormalMode {
        for (n, livecell_cnt, txs_cnt) in NORMAL_PHASE_CELLS_TXS_CNT.iter() {
            if height < n * MILLION_HEIGHT {
                return (*livecell_cnt, *txs_cnt);
            }
        }
        // reach end
        let (_, livecell_cnt, txs_cnt) = NORMAL_PHASE_CELLS_TXS_CNT.last().unwrap();
        (*livecell_cnt, *txs_cnt)
    } else {
        for (n, livecell_cnt, txs_cnt) in MAX_PHASE_CELLS_TXS_CNT.iter() {
            if height < n * MILLION_HEIGHT {
                return (*livecell_cnt, *txs_cnt);
            }
        }
        // reach end
        let (_, livecell_cnt, txs_cnt) = MAX_PHASE_CELLS_TXS_CNT.last().unwrap();
        (*livecell_cnt, *txs_cnt)
    }
}

/// get secp256k1 sighash CellDeps
pub fn secp256k1_cell_dep(genesis_block: &BlockView) -> Vec<CellDep> {
    let mut v = vec![];
    let op = OutPoint::new_builder()
        .tx_hash(
            genesis_block
                .transaction(1)
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
                .transaction(1)
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
    let tx_hash = tx.hash();
    let witness = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    let witness_len = witness.as_slice().len() as u64;
    let message = {
        let mut hasher = new_blake2b();
        hasher.update(tx_hash.as_slice());
        hasher.update(&witness_len.to_le_bytes());
        hasher.update(witness.as_slice());
        let mut buf = [0u8; 32];
        hasher.finalize(&mut buf);
        H256::from(buf)
    };

    for account in signed_accounts {
        let witness = WitnessArgs::new_builder()
            .lock(Some(Bytes::from(vec![0u8; 65])).pack())
            .build();
        let sig = account
            .private_key
            .sign_recoverable(&message)
            .expect("sign_recoverable");
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
pub fn gen_live_cells(
    parent: &BlockView,
    account: &mut Account,
    livecell_cnt: u64,
    secp_cell_deps: &Vec<CellDep>,
) -> TransactionView {
    let input = {
        let txs = parent.transactions();

        // if parent block is genesis, input cell is at tx_0 and len-1 index
        if parent.is_genesis() {
            let tx = txs.get(0).expect("get 1st live_cell transaction");
            CellInput::new(OutPoint::new(tx.hash(), 7), 0)
        } else {
            // the 2nd tx in parent block is input cell for this tx
            let tx = txs.get(1).expect("get live_cell transaction");
            CellInput::new(OutPoint::new(tx.hash(), 0), parent.header().number())
        }
    };

    // we keep capacity in this account cause it's simple
    let origin_cap = Capacity::zero()
        .safe_add(account.cell_cap)
        .expect("origin capacity");
    let rest = origin_cap
        .safe_sub(MIN_FEE_RATE as u64)
        .expect("for min_fee_rate");
    let cell_cap = Capacity::zero().safe_add(MIN_CELL_CAP).expect("cell_cap");
    let sum_cell_cap = cell_cap.safe_mul(livecell_cnt).expect("cell_cap multiple");
    let rest = rest
        .safe_sub(sum_cell_cap)
        .expect("sub live cells capacity");
    account.cell_cap = rest.as_u64();

    let mut outputs = vec![CellOutput::new_builder()
        .capacity(rest.as_u64().pack())
        .lock(account.lock_args.clone())
        .build()];
    (0..livecell_cnt).for_each(|_| {
        outputs.push(
            CellOutput::new_builder()
                .capacity(MIN_CELL_CAP.pack())
                .lock(account.lock_args.clone())
                .build(),
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

/// prepare input cells for 2in2out transactions
/// it will be called once at #20 height or every million height beginning
/// input cell is from previous million block output cell #0
/// output cells: #0...m-1(m==2in2out_tx_cnt * 2) is for 2in2out, #m is for next million input cell
fn prepare_two_two_txs(
    node: &Node,
    if_first: bool,
    owner_account: &mut Account,
    accounts: &mut [Account],
    txs_cnt: u64,
    secp_cell_deps: &Vec<CellDep>,
) -> TransactionView {
    let curr_height = node.get_tip_block_number() + 1;

    // get input cell capacity
    // fetch cell capacity from genesis tx or previous million height block tx
    let cell: CellWithStatus;
    let input: CellInput;

    if if_first {
        let genesis = node.get_block_by_number(0);
        let txs = genesis.transactions();
        let tx = txs.get(0).expect("get 1st tx");
        cell = node.rpc_client().get_live_cell(
            ckb_jsonrpc_types::OutPoint::from(OutPoint::new(tx.hash(), 8)),
            true,
        );
        input = CellInput::new(OutPoint::new(tx.hash(), 8), 0);
    } else {
        // Todo: replace with CellInput pushed in Vec when create, pop it when be used
        let previous_million_block = {
            if curr_height == MILLION_HEIGHT {
                node.get_block_by_number(TWO_TWO_START_HEIGHT)
            } else {
                node.get_block_by_number(curr_height - MILLION_HEIGHT)
            }
        };
        let txs = previous_million_block.transactions();
        let tx = txs.last().expect("get last tx");
        let last_output = tx.outputs().len() - 1;
        cell = node.rpc_client().get_live_cell(
            ckb_jsonrpc_types::OutPoint::from(OutPoint::new(tx.hash(), last_output as u32)),
            true,
        );
        input = CellInput::new(
            OutPoint::new(tx.hash(), last_output as u32),
            previous_million_block.header().number(),
        );
    }

    // subtract FEE_RATE and 2*txs_cnt cell's capacity
    let input_cell_capacity = cell.cell.expect("get cell info").output.capacity;

    let total = Capacity::zero()
        .safe_add(input_cell_capacity.value())
        .expect("origin capacity");
    let rest = total
        .safe_sub(MIN_FEE_RATE as u64)
        .expect("for min_fee_rate");
    let cellcap = Capacity::zero().safe_add(MIN_CELL_CAP).unwrap();
    let total_cellcap = cellcap.safe_mul(txs_cnt * 2).unwrap();
    let rest = rest.safe_sub(total_cellcap).expect("sub cells capacity");
    // accounts[0].cell_cap = rest.as_u64();
    owner_account.cell_cap = rest.as_u64();

    let mut outputs = vec![];
    // let owner_account = &accounts[0];

    for _ in 0..txs_cnt {
        let (input_accounts, _) = accounts.split_at(accounts.len() / 2);
        (0..2_usize).zip(input_accounts).for_each(|(_, account)| {
            outputs.push(
                CellOutput::new_builder()
                    .capacity(MIN_CELL_CAP.pack())
                    .lock(account.lock_args.clone())
                    .build(),
            );
        });
    }
    outputs.push(
        CellOutput::new_builder()
            .capacity(rest.as_u64().pack())
            .lock(owner_account.lock_args.clone())
            .build(),
    );

    let mut outputs_data = vec![];
    (0..=2 * txs_cnt as u16).for_each(|i| {
        outputs_data.push(Bytes::from(i.to_le_bytes().to_vec()));
    });

    let tx = TransactionBuilder::default()
        .input(input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_deps(secp_cell_deps.clone())
        .build();

    let accounts = [owner_account.clone()];
    attach_witness(tx, &accounts)
}

/// create 2in2out tx in expansion mode
pub fn create_two_two_txs(
    parent: &BlockView,
    accounts: &mut [Account],
    txs_cnt: u64,
    secp_cell_deps: &Vec<CellDep>,
) -> Vec<TransactionView> {
    let mut txs = vec![];

    //split accounts, [A, B, C, D] into [A, B] and [C, D]
    // [A, B] for 2 input cell of previous tx, and 2 output cells is locked by [C, D]
    let (input_acc, output_acc) = accounts.split_at(accounts.len() / 2);
    let parent_block_number = parent.header().number();

    for tx_index in 0..txs_cnt as usize {
        let inputs = {
            let p_txs = parent.transactions();
            if parent_block_number % MILLION_HEIGHT == 0 {
                // if current block is #21 or #million+1
                // the 2nd tx in parent block is input cell for two_two txs
                let tx = p_txs.last().expect("get previous transaction");
                vec![
                    CellInput::new(
                        OutPoint::new(tx.hash(), (2 * tx_index) as u32),
                        parent_block_number,
                    ),
                    CellInput::new(
                        OutPoint::new(tx.hash(), (2 * tx_index + 1) as u32),
                        parent_block_number,
                    ),
                ]
            } else {
                // from the 2nd..to End tx in parent block is input cell for two_two txs
                let tx = p_txs.get(tx_index + 2).expect("get previous transaction");
                vec![
                    CellInput::new(OutPoint::new(tx.hash(), 0), parent_block_number),
                    CellInput::new(OutPoint::new(tx.hash(), 1), parent_block_number),
                ]
            }
        };

        // we set fee_rate to zero
        // 2in2out input/output cell are always MIN_CELL_CAP
        let cell_cap = Capacity::zero()
            .safe_add(MIN_CELL_CAP)
            .expect("origin capacity");
        let rest = cell_cap
            .safe_sub(MIN_FEE_RATE as u64)
            .expect("for min_fee_rate");

        let outputs: Vec<CellOutput> = (0..2)
            .zip(output_acc.iter())
            .map(|(_, account)| {
                CellOutput::new_builder()
                    .capacity(rest.as_u64().pack())
                    .lock(account.lock_args.clone())
                    .build()
            })
            .collect();

        let mut outputs_data = vec![];
        (0_u8..2_u8).for_each(|i| {
            outputs_data.push(Bytes::from(i.to_le_bytes().to_vec()));
        });

        let tx = TransactionBuilder::default()
            .inputs(inputs)
            .outputs(outputs)
            .outputs_data(outputs_data.pack())
            .cell_deps(secp_cell_deps.clone())
            .build();

        // handle signature
        txs.push(attach_witness(tx, input_acc));
    }

    txs
}

fn main() -> std::io::Result<()> {
    let _logger = init_logger();
    let cli = CkbGrowth::parse();

    match &cli.sub_command {
        GrowthSubCommand::Run(matches) => cmd_run(matches),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ExpansionMode {
    NormalMode,
    MaximumMode,
}

fn cmd_run(matches: &CmdRun) -> std::io::Result<()> {
    let normal_mode = matches.normal_expansion;
    let maximum_mode = matches.maximum_expansion;
    let from = matches.from;
    let to = matches.to;

    if !normal_mode && !maximum_mode {
        eprintln!("need specific expansion mode: normal or maximum");
        exit(-1);
    }
    if normal_mode && maximum_mode {
        eprintln!("cannot use both mode, choose one expansion mode: normal or maximum");
        exit(-1);
    }
    if to < from {
        eprintln!("--to End_Block_Number, cannot less than --from Start_Block_Number");
        exit(-1);
    }
    if from != 0 && from < 20 {
        eprintln!("cannot specify `from` in 1..20");
        exit(-1);
    }

    let mode = {
        if normal_mode {
            ExpansionMode::NormalMode
        } else {
            ExpansionMode::MaximumMode
        }
    };

    if normal_mode {
        println!("normal mode in 5 years data expansion");
    } else {
        println!("maximum mode in 1 years data expansion");
    }

    expansion(mode, from, to)?;
    Ok(())
}

fn expansion(mode: ExpansionMode, from: u64, to: u64) -> std::io::Result<()> {
    let node = Node::new(PathBuf::from("./"));

    let genesis_block = node.get_block_by_number(0);
    let cell_dep = secp256k1_cell_dep(&genesis_block);

    let tip = node.get_tip_block_number();
    if (from == 0 && from != tip) || (from != 0 && from != (tip + 1)) {
        eprintln!(
            "generate blocks from {}, but mis-match with current tip {}",
            from, tip
        );
        exit(-1)
    }
    if to % MILLION_HEIGHT != 0 {
        eprintln!("--to {}, should be divided by 1 million whole ", to);
        exit(-1)
    }

    // mine first 19 blocks if generate from beginning
    let block_range = {
        if from == 0 {
            mine(&node, 19);
            20..=to
        } else {
            from..=to
        }
    };

    // the account embedded accounts in Dev chain
    // account for live cells generation
    let mut cellbase_account = Account::new(
        h256!("0xd00c06bfd800d27397002dca6fb0993d5ba6399b4238b2f29ee9deb97593d2bc"),
        2_000_000_000_000_000_000,
    );

    // the account embedded accounts in Dev chain
    // base account, derive more accounts for building 2in2out tx
    let mut owner_account = Account::new(
        h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
        519_873_503_700_000_000,
    );

    //load account cell capacity info from serialization file if --from is not 0
    if from != 0 {
        let mut f = OpenOptions::new()
            .read(true)
            .open("account_cellcap.dat")
            .expect("load account cell cap file error");
        let mut cap_data = String::new();
        f.read_to_string(&mut cap_data)?;
        let cellcap: AccountCellCap =
            serde_json::from_str(cap_data.as_str()).expect("Deserialised from account_cellcap.dat");
        cellbase_account.cell_cap = cellcap.cellbase_cap;
        owner_account.cell_cap = cellcap.owner_cap;
    }

    // prepare 4 accounts and put them into 2in2out_accounts
    let mut two_two_accounts = vec![owner_account.clone()];
    for i in 0..4 {
        let new_account = two_two_accounts[i].derive_new_account();
        two_two_accounts.push(new_account);
    }

    let (mut livecell_cnt, mut txs_cnt) = get_livecellcnt_txcnt(mode, *block_range.start());

    for height in block_range {
        // prepare check point
        if (height == 20) || (height % MILLION_HEIGHT) == 0 {
            debug!("preparing job at height:{}", height);
            prepare_job_each_million(
                mode,
                &node,
                &mut cellbase_account,
                &mut owner_account,
                &mut two_two_accounts,
                &cell_dep,
            );

            // update livecell count and 2in2out txs count for next million
            (livecell_cnt, txs_cnt) = get_livecellcnt_txcnt(mode, height + 1);

            // save account info at every million height
            save_account_cellcap_to_file(&cellbase_account, &owner_account, &two_two_accounts)?;
        } else {
            let parent = node.get_tip_block();
            let block = node.new_block(None, None, None);

            debug!("processing txs and block at height:{}", height);

            let live_cells_tx =
                gen_live_cells(&parent, &mut cellbase_account, livecell_cnt, &cell_dep);

            let two_two_txs =
                create_two_two_txs(&parent, &mut two_two_accounts, txs_cnt, &cell_dep);

            let builder = block
                .as_advanced_builder()
                .transactions(vec![live_cells_tx])
                .transactions(two_two_txs);

            //disable verify, submit block
            node.process_block_without_verify(&builder.build(), false);

            // prepare for next transfer cell back
            revert_two_two_accounts(&mut two_two_accounts);
        }
    }

    Ok(())
}

/// turn [A, B, C, D] into [C, D, A, B], vice versa
fn revert_two_two_accounts(two_two_accounts: &mut [Account]) {
    two_two_accounts.swap(0, 2);
    two_two_accounts.swap(1, 3);
}

/// save account cellcap info to file at every million height
/// in case of pause and re-run
fn save_account_cellcap_to_file(
    cellbase_account: &Account,
    owner_account: &Account,
    two_two_accounts: &[Account],
) -> std::io::Result<()> {
    // serialize account cell cap info into file
    let cellcap = AccountCellCap {
        cellbase_cap: cellbase_account.cell_cap,
        owner_cap: owner_account.cell_cap,
        owner_derived_cap: (
            two_two_accounts[1].cell_cap,
            two_two_accounts[2].cell_cap,
            two_two_accounts[3].cell_cap,
        ),
    };
    let content = serde_json::to_string(&cellcap).expect("erialize account cell cap");
    let mut save = OpenOptions::new()
        .write(true)
        .create(true)
        .open("account_cellcap.dat")
        .expect("load account cell cap file error");
    save.write_all(content.as_ref())?;
    Ok(())
}

/// preparation job at block #20 and each million block
fn prepare_job_each_million(
    mode: ExpansionMode,
    node: &Node,
    cellbase_account: &mut Account,
    owner_account: &mut Account,
    two_two_accounts: &mut [Account],
    cell_dep: &Vec<CellDep>,
) {
    let parent_block = node.get_tip_block();
    let current_height = parent_block.number() + 1;
    let live_cells_tx: TransactionView;
    let prepare_2in2out: TransactionView;

    // double check if preparation job needs to be done
    // at height #20 or at each million height
    if (current_height != 20) && (current_height % MILLION_HEIGHT) != 0 {
        return;
    }

    let (livecell_cnt, txs_cnt) = get_livecellcnt_txcnt(mode, current_height + 1);

    if current_height == 20 {
        // prepare gen_live_cells
        let genesis_block = node.get_block_by_number(0);
        live_cells_tx = gen_live_cells(&genesis_block, cellbase_account, livecell_cnt, cell_dep);

        // prepare 2in2out input cells
        prepare_2in2out = prepare_two_two_txs(
            node,
            true,
            owner_account,
            two_two_accounts,
            txs_cnt,
            cell_dep,
        );
    } else {
        // prepare gen_live_cells
        live_cells_tx = gen_live_cells(&parent_block, cellbase_account, livecell_cnt, cell_dep);

        // revert two_two_accounts when at million height
        // so make it as [A, B, C, D] as original, for function pause/re-run
        revert_two_two_accounts(two_two_accounts);

        // prepare 2in2out input cells
        prepare_2in2out = prepare_two_two_txs(
            node,
            false,
            owner_account,
            two_two_accounts,
            txs_cnt,
            cell_dep,
        );
    }

    let block = node.new_block(None, None, None);
    let builder = block
        .as_advanced_builder()
        .transactions(vec![live_cells_tx, prepare_2in2out]);

    node.process_block_without_verify(&builder.build(), false);
}

fn init_logger() -> ckb_logger_service::LoggerInitGuard {
    let filter = match env::var("RUST_LOG") {
        Ok(filter) if filter.is_empty() => Some("info".to_string()),
        Ok(filter) => Some(filter),
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
