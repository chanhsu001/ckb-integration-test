mod util;

use crate::util::{create_2in2out_transactions, create_input_cells_in_normal_mode, gen_live_cells, gen_secp_block, new_secp_dev_chain, new_secp_dev_chain_raw};
use crate::util::{Account, Chain};
use ckb_async_runtime::new_global_runtime;
use ckb_chain_spec::DepGroupResource;
use ckb_growth::MAX_TXS_IN_NORMAL_MODE;
use ckb_resource::Resource;
use ckb_store::{self, ChainStore};
use ckb_types::core::{BlockView, DepType, TransactionView};
use ckb_types::packed::{CellDep, OutPoint, ProposalShortId, TransactionViewBuilder};
use ckb_types::prelude::{Builder, Entity, Unpack};
use ckb_types::{h160, h256};
use ckb_verification::SinceMetric::BlockNumber;
use clap::{Args, Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

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

pub fn single_secp256k1_cell_dep(genesis_block: &BlockView) -> CellDep {
    use ckb_types::prelude::Pack;
    let op = OutPoint::new_builder()
        .tx_hash(
            genesis_block
                .transaction(1)
                .expect("index genesis dep-group transaction")
                .hash(),
        )
        .index(0_usize.pack())
        .build();
    CellDep::new_builder()
        .out_point(op)
        .dep_type(DepType::DepGroup.into())
        .build()
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

/// normal expansion, design livecell tx and transfer tx in 3-blocks-group
fn normal_expansion(data_dir: &PathBuf) {
    let (handle, _) = new_global_runtime();

    let owner_account =
        // the account embedded accounts in Dev chain
        Account::new(
            h256!("0xd00c06bfd800d27397002dca6fb0993d5ba6399b4238b2f29ee9deb97593d2bc"),
            h160!("0xc8328aabcd9b9e8e64fbc566c4385c3bdeb219d7"),
            20_000_000_000_00000000,
        );
    // Account::new(
    //     h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
    //     h160!("0x470dcdc5e44064909650113a274b3b36aecb6dc7"),
    //     5_198_735_037_00000000,
    // ),

    let cellbase_account =
        // primary cellbase account
        Account::new(
            h256!("0xb2b3324cece882bca684eaf202667bb56ed8e8c2fd4b4dc71f615ebd6d9055a5"),
            h160!("0x779e5930892a0a9bf2fedfe048f685466c7d0396"),
            0
        );
    // Account::new(
    //     h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
    //     h160!("0x470dcdc5e44064909650113a274b3b36aecb6dc7"),
    //     5_198_735_037_00000000,
    // );

    let secp_cell_deps = vec![
        DepGroupResource {
            name: "secp_data".to_string(),
            files: vec![Resource::bundled(String::from(
                "specs/cells/secp256k1_data",
            ))],
        },
        DepGroupResource {
            name: "secp".to_string(),
            files: vec![Resource::bundled(String::from(
                "specs/cells/secp256k1_blake160_sighash_all",
            ))],
        },
    ];

    // all 2in2out transfer transaction only happens in 4 accounts
    let mut transfer_accounts = vec![cellbase_account.clone()];
    let base_account = &transfer_accounts[0];
    for i in 1..=3 {
        // for easy implementation, we change derived accounts to same account
        // let account = base_account.derive_new_account();
        // transfer_accounts.push(account);
        // base_account = &transfer_acounts[i];
        transfer_accounts.push(cellbase_account.clone());
    }

    // let chain = new_secp_dev_chain(&data_dir, handle);
    let Chain {controller, shared} = new_secp_dev_chain_raw(&owner_account);
    // let controller = chain.controller.clone();
    // let shared = chain.shared.clone();

    let genesis_block = shared.snapshot().get_block(&shared.genesis_hash()).unwrap();
    let mut parent = genesis_block.clone();
    let cell_dep = secp256k1_cell_dep(&genesis_block);

    // enough cellbase tx for cellbase account and pass genesis account spend block-limit
    let mut cellbase_info = vec![];
    (1..=12).for_each(|_| {
        let block = gen_secp_block(&parent, &shared, &cellbase_account, false, vec![], vec![]);
        controller
            .process_block(Arc::new(block.clone()))
            .expect("process block OK");
        parent = block;
    });

    (13..=17).for_each(|_| {
        let block = gen_secp_block(&parent, &shared, &cellbase_account, true, vec![], vec![]);
        controller
            .process_block(Arc::new(block.clone()))
            .expect("process block OK");
        cellbase_info.push((block.transactions()[0].hash(), block.header().number()));
        parent = block;
    });

    // 18, 19, 20 block, generate 2in2out input cells for next all blocks
    let input_cells_tx =
        create_input_cells_in_normal_mode(&cellbase_account, &cellbase_info, &cell_dep);
    (18..19).for_each(|_| {
        let block = gen_secp_block(
            &parent,
            &shared,
            &cellbase_account,
            true,
            vec![input_cells_tx.proposal_short_id()],
            vec![],
        );
        controller
            .process_block(Arc::new(block.clone()))
            .expect("process block OK");
        parent = block;
    });
    // #19 block
    {
        let block = gen_secp_block(&parent, &shared, &cellbase_account, true, vec![], vec![]);
        controller
            .process_block(Arc::new(block.clone()))
            .expect("process block OK");
        parent = block;
    }
    // #20 block
    {
        let block = gen_secp_block(
            &parent,
            &shared,
            &cellbase_account,
            true,
            vec![],
            vec![input_cells_tx.clone()],
        );
        controller
            .process_block(Arc::new(block.clone()))
            .expect("process block OK");
        parent = block;
    }

    // 21..=23 block
    let height = 21;
    let (livecell_cnt, txs_cnt) = get_livecellcnt_txcnt(height);
    // #21
    let live_cells_tx = gen_live_cells(&parent, &owner_account, 21, livecell_cnt);
    let (inputs_accounts, output_accounts) =
        transfer_accounts.split_at(transfer_accounts.len() / 2);
    let transfer_txs = create_2in2out_transactions(
        &parent,
        inputs_accounts,
        output_accounts,
        txs_cnt as u32,
        &cell_dep,
    );
    let mut proposals = vec![live_cells_tx.proposal_short_id()];
    proposals.extend(transfer_txs.iter().map(|tx| tx.proposal_short_id()));
    let block = gen_secp_block(&parent, &shared, &cellbase_account, true, proposals, vec![]);
    controller
        .process_block(Arc::new(block.clone()))
        .expect("process block OK");
    parent = block;

    //#22
    let block = gen_secp_block(&parent, &shared, &cellbase_account, true, vec![], vec![]);
    controller
        .process_block(Arc::new(block.clone()))
        .expect("process block OK");
    parent = block;

    //#23
    let mut txs = vec![live_cells_tx];
    txs.extend(transfer_txs);
    let block = gen_secp_block(&parent, &shared, &cellbase_account, true, vec![], vec![]);
    controller
        .process_block(Arc::new(block.clone()))
        .expect("process block OK");
    parent = block;

    // revert transfer_accounts for next 3 blocks
    // this cycle, input: A B ,output: C, D
    // next cycle, input: C D ,output: A, B
    let mut new_accounts = Vec::new();
    new_accounts.extend_from_slice(output_accounts);
    new_accounts.extend_from_slice(inputs_accounts);
    transfer_accounts = new_accounts;
}

// fn max_expansion(data_dir: &PathBuf, interval: Duration) {
//     let (handle, _) = new_global_runtime();
//
//     let owner_account = vec![
//         // the account embedded accounts in Dev chain
//         Account::new(
//             h256!("0xd00c06bfd800d27397002dca6fb0993d5ba6399b4238b2f29ee9deb97593d2bc"),
//             h160!("0xc8328aabcd9b9e8e64fbc566c4385c3bdeb219d7"),
//             20_000_000_000_00000000,
//         ),
//         // Account::new(
//         //     h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
//         //     h160!("0x470dcdc5e44064909650113a274b3b36aecb6dc7"),
//         //     5_198_735_037_00000000,
//         // ),
//     ];
//     let mut cellbase_account =
//         // primary cellbase account
//         Account::new(
//             h256!("0xb2b3324cece882bca684eaf202667bb56ed8e8c2fd4b4dc71f615ebd6d9055a5"),
//             h160!("0x779e5930892a0a9bf2fedfe048f685466c7d0396"),
//             0
//         );
//
//     // all 2in2out transfer transaction only happens in 4 accounts
//     let mut transfer_acounts = vec![cellbase_account.clone()];
//     let mut base_account = &transfer_acounts[0];
//     for i in 1..=3 {
//         let account = base_account.derive_new_account();
//         transfer_acounts.push(account);
//         base_account = &transfer_acounts[i];
//     }
//
//     let Chain((chain, shared)) = new_secp_dev_chain(&data_dir, t_tx_interval, handle);
//
//     let genesis_block = shared.snapshot().get_block(&shared.genesis_hash()).unwrap();
//     let mut parent = genesis_block.clone();
//
//     // enough cellbase tx for cellbase account and pass genesis account spend block-limit
//     (1..=20).for_each(|_| {
//         let cellbase_tx = create_secp_cellbase(&shared, &parent.header(), &mut cellbase_account);
//         let block = gen_secp_block(&parent, &shared, vec![], cellbase_tx, vec![]);
//         chain
//             .process_block(Arc::new(block.clone()))
//             .expect("process block OK");
//         parent = block;
//     });
//
//     let height = 21;
//     let (livecell_cnt, txs_cnt) = get_livecellcnt_txcnt(height);
//
//     // 6 blocks into one group, generate live cell txs and transfer txs
//     let mut proposal_txs: Vec<ProposalShortId> = vec![];
//     let mut transfer_txs: Vec<TransactionView> = vec![];
//     let mut livecell_tx: TransactionView = TransactionViewBuilder::default().build().unpack();
//     (1..=6).for_each(|i| {
//         // proposal one livecell tx at 1st block for 1..6 blocks and commit at 6nd block
//         if i == 1 {
//             //setup
//             let mut livecell_proposal = vec![];
//             transfer_txs = vec![];
//
//             livecell_tx = gen_live_cells(&parent, &owner_account[0], height, livecell_cnt * 6);
//             livecell_proposal.push(livecell_tx.proposal_short_id());
//
//             let block = gen_secp_block(
//                 &parent,
//                 &shared,
//                 cellbase_tx,
//                 transfer_txs.clone(),
//             );
//             chain
//                 .process_block(Arc::new(block.clone()))
//                 .expect("process block OK");
//             parent = block.clone();
//         }
//
//         if i == 6 {
//             let block = gen_secp_block(
//                 &parent,
//                 &shared,
//                 proposal_txs.clone(),
//                 transfer_txs.clone(),
//             );
//             chain
//                 .process_block(Arc::new(block.clone()))
//                 .expect("process block OK");
//             parent = block.clone();
//         }
//     });
// }

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
