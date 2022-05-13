use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;

use ckb_jsonrpc_types::{CellWithStatus, Deserialize, Serialize};
use ckb_types::{
    bytes::Bytes,
    core::{BlockView, Capacity, TransactionBuilder, TransactionView},
    packed::{CellDep, CellInput, CellOutput, OutPoint},
    prelude::*,
};

use growth_utils::{
    Account, attach_witness, genesis_block_1tx_8output_as_new_input,
    MIN_CELL_CAP, MIN_FEE_RATE, parent_block_2tx_1output_as_new_input,
};
use growth_utils::node::Node;

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountCellCap {
    pub cellbase_cap: u64,
    pub owner_cap: u64,
    pub owner_derived_cap: (u64, u64, u64),
}

/// save account cellcap info to file at every million height
/// in case of pause and re-run
pub fn load_account_cellcap(
    file: &PathBuf,
    cellbase_account: &mut Account,
    owner_account: &mut Account,
    derived_two_two_accounts: &mut [Account],
) {
    let mut f = OpenOptions::new()
        .read(true)
        .open(file)
        .expect("load account cell cap file error");
    let mut cap_data = String::new();
    f.read_to_string(&mut cap_data)
        .expect("read_to_string error");
    let cellcap: AccountCellCap =
        serde_json::from_str(cap_data.as_str()).expect("Deserialised from account_cellcap.dat");
    cellbase_account.cell_cap = cellcap.cellbase_cap;
    owner_account.cell_cap = cellcap.owner_cap;
    derived_two_two_accounts[1].cell_cap = cellcap.owner_derived_cap.0;
    derived_two_two_accounts[2].cell_cap = cellcap.owner_derived_cap.1;
    derived_two_two_accounts[3].cell_cap = cellcap.owner_derived_cap.2;
}

/// save account cellcap info to file at every million height
/// in case of pause and re-run
pub fn save_account_cellcap_to_file(
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

pub const TWO_TWO_START_HEIGHT: u64 = 20;
pub const MILLION_HEIGHT: u64 = 1_000_000;

pub type MillionHeight = u64;
pub type LiveCellCnt = u64;
pub type TxCnt = u64;

pub const MAX_TXS_IN_NORMAL_MODE: u64 = 9;
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

#[derive(Clone, Copy, PartialEq)]
pub enum ExpansionMode {
    NormalMode,
    MaximumMode,
}

/// return each block should contains livecells count and transfer-txs count at specific height
pub fn get_livecellcnt_txcnt(mode: ExpansionMode, height: u64) -> (LiveCellCnt, TxCnt) {
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

/// build 1in-Nout transaction to create N output_cell out of 1 input_cell on one account
/// the 1st cell capacity is nearly equal to input cell, the other cells capacity is tiny
pub fn gen_live_cells(
    input: CellInput,
    account: &mut Account,
    livecell_cnt: u64,
    secp_cell_deps: &[CellDep],
) -> TransactionView {
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

    let secp_cell_deps = Vec::from(secp_cell_deps);
    let tx = TransactionBuilder::default()
        .input(input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_deps(secp_cell_deps)
        .build();
    let accounts = [account.clone()];
    attach_witness(tx, &accounts)
}

/// prepare input cells for 2in2out transactions
/// it will be called once at #20 height or every million height beginning
/// input cell is from previous million block output cell #0
/// output cells: #0...m-1(m==2in2out_tx_cnt * 2) is for 2in2out, #m is for next million input cell
pub fn prepare_two_two_txs(
    node: &Node,
    if_first: bool,
    owner_account: &mut Account,
    accounts: &mut [Account],
    txs_cnt: u64,
    secp_cell_deps: &[CellDep],
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

    let secp_cell_deps = Vec::from(secp_cell_deps);
    let tx = TransactionBuilder::default()
        .input(input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_deps(secp_cell_deps)
        .build();

    let accounts = [owner_account.clone()];
    attach_witness(tx, &accounts)
}

/// create 2in2out tx in expansion mode
pub fn create_two_two_txs(
    parent: &BlockView,
    accounts: &mut [Account],
    txs_cnt: u64,
    secp_cell_deps: &[CellDep],
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

        let secp_cell_deps = Vec::from(secp_cell_deps);
        let tx = TransactionBuilder::default()
            .inputs(inputs)
            .outputs(outputs)
            .outputs_data(outputs_data.pack())
            .cell_deps(secp_cell_deps)
            .build();

        // handle signature
        txs.push(attach_witness(tx, input_acc));
    }

    txs
}

/// turn [A, B, C, D] into [C, D, A, B], vice versa
pub fn revert_two_two_accounts(two_two_accounts: &mut [Account]) {
    two_two_accounts.swap(0, 2);
    two_two_accounts.swap(1, 3);
}

/// preparation job at block #20 and each million block
pub fn prepare_job_each_million(
    mode: ExpansionMode,
    node: &Node,
    cellbase_account: &mut Account,
    owner_account: &mut Account,
    two_two_accounts: &mut [Account],
    cell_dep: &[CellDep],
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
        let input = genesis_block_1tx_8output_as_new_input(node);
        live_cells_tx = gen_live_cells(input, cellbase_account, livecell_cnt, cell_dep);

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
        let input = parent_block_2tx_1output_as_new_input(node);
        live_cells_tx = gen_live_cells(input, cellbase_account, livecell_cnt, cell_dep);

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
