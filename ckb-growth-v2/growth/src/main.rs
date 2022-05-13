extern crate core;

use std::env;
use std::panic;
use std::path::PathBuf;
use std::process::exit;

use ckb_logger::debug;
use ckb_types::h256;
use clap::{Args, Parser, Subcommand};

use growth::save_account_cellcap_to_file;
use growth::{
    create_two_two_txs, gen_live_cells, get_livecellcnt_txcnt, load_account_cellcap,
    prepare_job_each_million, revert_two_two_accounts, ExpansionMode, MILLION_HEIGHT,
};
use growth_utils::mining::mine;
use growth_utils::node::Node;
use growth_utils::{parent_block_2tx_1output_as_new_input, secp256k1_cell_dep, Account};

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

fn main() -> std::io::Result<()> {
    let _logger = init_logger();
    let cli = CkbGrowth::parse();

    match &cli.sub_command {
        GrowthSubCommand::Run(matches) => cmd_run(matches),
    }
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

    // prepare 4 accounts and put them into 2in2out_accounts
    let mut two_two_accounts = vec![owner_account.clone()];
    for i in 0..4 {
        let new_account = two_two_accounts[i].derive_new_account();
        two_two_accounts.push(new_account);
    }

    //load account cell capacity info from serialization file if --from is not 0
    if from != 0 {
        let file = PathBuf::from("account_cellcap.dat");
        load_account_cellcap(
            &file,
            &mut cellbase_account,
            &mut owner_account,
            &mut two_two_accounts,
        );
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

            let input = parent_block_2tx_1output_as_new_input(&node);

            let live_cells_tx =
                gen_live_cells(input, &mut cellbase_account, livecell_cnt, &cell_dep);

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
