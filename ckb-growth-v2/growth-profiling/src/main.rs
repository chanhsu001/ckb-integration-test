use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use ckb_types::h256;
use ckb_types::packed::{CellDep, CellInput, OutPoint, ProposalShortId};
use clap::{Args, Parser, Subcommand};
use rand::Rng;

use growth_profiling::{create_2in2out_txs, gen_live_cells, generate_accounts};
use growth_utils::mining::mine;
use growth_utils::node::Node;
use growth_utils::{
    load_accounts_from_file, save_accounts_cellcap_to_file, secp256k1_cell_dep, Account,
};

#[derive(Parser)]
#[clap(
    name = "ckb_growth_profiling",
    author = "Nervos Core Dev <dev@nervos.org>",
    about = "Nervos CKB - The Common Knowledge Base"
)]
/// command line structure for clap parsed
pub struct CkbGrowthProfile {
    /// ckb subcommand
    #[clap(subcommand)]
    pub sub_command: GrowthProfileSubCommand,
}

#[derive(Subcommand)]
#[clap()]
/// ckb subcommand
pub enum GrowthProfileSubCommand {
    /// sequence fetch blocks subcommand
    #[clap(about = "Sequence Rpc fetch blocks")]
    Seq(CmdSeqFetch),
    /// random fetch blocks subcommand
    #[clap(about = "Random Rpc fetch blocks")]
    Random(CmdRanFetch),
    /// process full blocks subcommand
    #[clap(about = "Process Blocks full with 2in2out Txs")]
    Process(CmdBlockProcess),
    /// generate full blocks subcommand
    #[clap(about = "Generate Blocks full with 2in2out Txs")]
    Generate(CmdBlockGenerate),
}

#[derive(Args)]
#[clap()]
pub struct CmdSeqFetch {
    #[clap(short, long, default_value_t = 1)]
    /// Specifies starting index of block range
    from: usize,

    #[clap(short, long)]
    /// Specifies ending index of block range
    to: usize,

    #[clap(short, long, default_value_t = 10_000)]
    /// Specifies number of blocks to fetch
    block_cnt: usize,
}

#[derive(Args)]
#[clap()]
pub struct CmdRanFetch {
    #[clap(short, long, default_value_t = 1)]
    /// Specifies starting index of block range
    from: usize,

    #[clap(short, long)]
    /// Specifies ending index of block range
    to: usize,

    #[clap(short, long, default_value_t = 10_000)]
    /// Specifies number of blocks to fetch
    block_cnt: usize,
}

#[derive(Args)]
#[clap()]
pub struct CmdBlockProcess {}

#[derive(Args)]
#[clap()]
pub struct CmdBlockGenerate {}

fn main() {
    let cli = CkbGrowthProfile::parse();

    match &cli.sub_command {
        GrowthProfileSubCommand::Seq(matches) => {
            let node = Node::new(PathBuf::from("./"));
            let block_range = matches.from..matches.to;
            let profile = seq_fetch_blocks(&node, matches.block_cnt, block_range);
            println!(
                "Sequence fetch {} blocks takes: {} seconds",
                matches.block_cnt, profile
            );
        }
        GrowthProfileSubCommand::Random(matches) => {
            let node = Node::new(PathBuf::from("./"));
            let block_range = matches.from..matches.to;
            let profile = random_fetch_blocks(&node, matches.block_cnt, block_range);
            println!(
                "Random fetch {} blocks takes: {} seconds",
                matches.block_cnt, profile
            );
        }
        // fullblock profiling is done at ckb side
        GrowthProfileSubCommand::Generate(_) => full_block_generate(),
        GrowthProfileSubCommand::Process(_) => full_block_process(),
    }
}

fn seq_fetch_blocks(node: &Node, block_cnt: usize, block_range: Range<usize>) -> u64 {
    let mut rng = rand::thread_rng();
    let mut start: usize;
    //random select start index
    loop {
        start = rng.gen_range(block_range.start, block_range.end) as usize;
        if start + block_cnt < block_range.end {
            break;
        }
    }

    let now = Instant::now();
    for index in start..=start + block_cnt {
        if node
            .rpc_client()
            .get_block_by_number(index as u64)
            .is_none()
        {
            panic!("get block number:{} error!", index);
        }
    }
    now.elapsed().as_secs()
}

fn random_fetch_blocks(node: &Node, block_cnt: usize, block_range: Range<usize>) -> u64 {
    let mut rng = rand::thread_rng();
    //pre-create random fetch index in case of profiling
    let v: Vec<usize> = (0..block_cnt)
        .map(|_| rng.gen_range(block_range.start, block_range.end))
        .collect();

    let now = Instant::now();
    for index in v.iter() {
        if node
            .rpc_client()
            .get_block_by_number(*index as u64)
            .is_none()
        {
            panic!("get block number:{} error!", index);
        }
    }
    now.elapsed().as_secs()
}

const TXS_CNT: u16 = 917;

//mine 1-12 blocks
//block 13 propose live_cell tx
//block 14 mine
//block 15 wraps live_cell tx
//==============================
//block 16 propose 2in2out tx
//block 17 mine
//block 18 wraps 2in2out tx
/// note: the block and txs should valid and pass other nodes validation
fn full_block_generate() {
    let node = Node::new(PathBuf::from("./"));

    let genesis_block = node.get_block_by_number(0);
    let cell_dep = secp256k1_cell_dep(&genesis_block);

    // the account embedded accounts in Dev chain
    let owner_account = Account::new(
        h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
        519_873_503_700_000_000,
    );
    let mut accounts = generate_accounts(owner_account, 2 * TXS_CNT);
    let account_file = PathBuf::from("account_cellcap.dat");

    prepare_job(&node, &mut accounts, &account_file, &cell_dep);
}

/// prepare for fullblock
fn prepare_job(node: &Node, accounts: &mut [Account], file: &PathBuf, cell_dep: &[CellDep]) {
    let genesis_block = node.get_block_by_number(0);

    mine(node, 12);

    // input cell is at tx_0 and len-1 index on genesis block
    let input = {
        let txs = genesis_block.transactions();
        let tx = txs.get(0).expect("get 1st live_cell transaction");
        CellInput::new(OutPoint::new(tx.hash(), 8), 0)
    };
    let cells_tx = gen_live_cells(input, accounts, 2 * TXS_CNT, cell_dep);
    // #13
    {
        let block = node.new_block(None, None, None);
        let builder = block
            .as_advanced_builder()
            .proposal(cells_tx.proposal_short_id());
        node.submit_block(&builder.build());
    }
    // #14, 15
    node.submit_transaction(&cells_tx);
    mine(node, 1);
    mine(node, 1);

    save_accounts_cellcap_to_file(accounts, file);
}

/// fullblock process
fn commit_full_block(node: &Node, accounts: &mut [Account], file: &PathBuf, cell_dep: &[CellDep]) {
    load_accounts_from_file(accounts, file);

    let inputs: Vec<CellInput> = {
        let parent_block = node.get_tip_block();
        let parent_block_number = parent_block.number();
        let live_cells_tx_hash = parent_block
            .transactions()
            .last()
            .expect("get last tx of parent block")
            .hash();

        (1..=2 * TXS_CNT)
            .into_iter()
            .map(|i| {
                CellInput::new(
                    OutPoint::new(live_cells_tx_hash.clone(), i as u32),
                    parent_block_number,
                )
            })
            .collect()
    };
    let twotwo_txs = create_2in2out_txs(inputs, accounts, TXS_CNT, cell_dep);
    // #16
    {
        let proposals = {
            twotwo_txs
                .iter()
                .map(|tx| tx.proposal_short_id())
                .collect::<Vec<ProposalShortId>>()
        };

        let block = node.new_block(None, None, None);
        let builder = block.as_advanced_builder().proposals(proposals);
        node.submit_block(&builder.build());
    }

    for tx in twotwo_txs.iter() {
        node.submit_transaction(tx);
    }

    // #17, 18
    mine(node, 1);
    mine(node, 1);
}

fn full_block_process() {
    let node = Node::new(PathBuf::from("./"));

    let genesis_block = node.get_block_by_number(0);
    let cell_dep = secp256k1_cell_dep(&genesis_block);

    // the account embedded accounts in Dev chain
    let owner_account = Account::new(
        h256!("0x63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d"),
        519_873_503_700_000_000,
    );
    let mut accounts = generate_accounts(owner_account, 2 * TXS_CNT);

    // prepare checkgen_live_cells point
    let account_file = PathBuf::from("account_cellcap.dat");
    commit_full_block(&node, &mut accounts, &account_file, &cell_dep);
}
