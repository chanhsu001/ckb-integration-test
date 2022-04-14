use ckb_network::bytes::Bytes;
use ckb_types::{
    core::{BlockNumber, BlockView, EpochNumberWithFraction, HeaderView, TransactionView},
    packed::{
        BlockTransactions, Byte32, CompactBlock, GetBlocks, RelayMessage, RelayTransaction,
        RelayTransactionHashes, RelayTransactions, SendBlock, SendHeaders, SyncMessage,
    },
    prelude::*,
};
use core::sync::atomic::Ordering::SeqCst;
use std::convert::Into;
use std::env;
use std::fs::read_to_string;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ckb_jsonrpc_types::Status;
use lazy_static::lazy_static;
use crate::node::Node;
use std::sync::atomic::AtomicU16;

lazy_static! {
    pub static ref PORT_COUNTER: AtomicU16 = AtomicU16::new(9000);
}

// Build compact block based on core block, and specific prefilled indices
pub fn build_compact_block_with_prefilled(block: &BlockView, prefilled: Vec<usize>) -> Bytes {
    let prefilled = prefilled.into_iter().collect();
    let compact_block = CompactBlock::build_from_block(block, &prefilled);

    RelayMessage::new_builder()
        .set(compact_block)
        .build()
        .as_bytes()
}

// Build compact block based on core block
pub fn build_compact_block(block: &BlockView) -> Bytes {
    build_compact_block_with_prefilled(block, Vec::new())
}

pub fn build_block_transactions(block: &BlockView) -> Bytes {
    // compact block has always prefilled cellbase
    let block_txs = BlockTransactions::new_builder()
        .block_hash(block.header().hash())
        .transactions(
            block
                .transactions()
                .into_iter()
                .map(|view| view.data())
                .skip(1)
                .pack(),
        )
        .build();

    RelayMessage::new_builder()
        .set(block_txs)
        .build()
        .as_bytes()
}

pub fn build_header(header: &HeaderView) -> Bytes {
    build_headers(&[header.clone()])
}

pub fn build_headers(headers: &[HeaderView]) -> Bytes {
    let send_headers = SendHeaders::new_builder()
        .headers(
            headers
                .iter()
                .map(|view| view.data())
                .collect::<Vec<_>>()
                .pack(),
        )
        .build();

    SyncMessage::new_builder()
        .set(send_headers)
        .build()
        .as_bytes()
}

pub fn build_block(block: &BlockView) -> Bytes {
    SyncMessage::new_builder()
        .set(SendBlock::new_builder().block(block.data()).build())
        .build()
        .as_bytes()
}

pub fn build_get_blocks(hashes: &[Byte32]) -> Bytes {
    let get_blocks = GetBlocks::new_builder()
        .block_hashes(hashes.iter().map(ToOwned::to_owned).pack())
        .build();

    SyncMessage::new_builder()
        .set(get_blocks)
        .build()
        .as_bytes()
}

pub fn build_relay_txs(transactions: &[(TransactionView, u64)]) -> Bytes {
    let transactions = transactions.iter().map(|(tx, cycles)| {
        RelayTransaction::new_builder()
            .cycles(cycles.pack())
            .transaction(tx.data())
            .build()
    });
    let txs = RelayTransactions::new_builder()
        .transactions(transactions.pack())
        .build();

    RelayMessage::new_builder().set(txs).build().as_bytes()
}

pub fn build_relay_tx_hashes(hashes: &[Byte32]) -> Bytes {
    let content = RelayTransactionHashes::new_builder()
        .tx_hashes(hashes.iter().map(ToOwned::to_owned).pack())
        .build();

    RelayMessage::new_builder().set(content).build().as_bytes()
}

pub fn wait_until<F>(secs: u64, mut f: F) -> bool
where
    F: FnMut() -> bool,
{
    let timeout = tweaked_duration(secs);
    let start = Instant::now();
    while Instant::now().duration_since(start) <= timeout {
        if f() {
            return true;
        }
        thread::sleep(Duration::new(1, 0));
    }
    false
}

pub fn sleep(secs: u64) {
    thread::sleep(tweaked_duration(secs));
}

pub fn tweaked_duration(secs: u64) -> Duration {
    let sec_coefficient = env::var("CKB_TEST_SEC_COEFFICIENT")
        .unwrap_or_default()
        .parse()
        .unwrap_or(1.0);
    Duration::from_secs((secs as f64 * sec_coefficient) as u64)
}

pub fn assert_send_transaction_fail(node: &Node, transaction: &TransactionView, message: &str) {
    let result = node
        .rpc_client()
        .send_transaction_result(transaction.data().into());
    assert!(
        result.is_err(),
        "expect error \"{}\" but got \"Ok(())\"",
        message,
    );
    let error = result.expect_err(&format!("transaction is invalid since {}", message));
    let error_string = error.to_string();
    assert!(
        error_string.contains(message),
        "expect error \"{}\" but got \"{}\"",
        message,
        error_string,
    );
}

// /// Return a random path located on temp_dir
// ///
// /// We use `tempdir` only for generating a random path, and expect the corresponding directory
// /// that `tempdir` creates be deleted when go out of this function.
// pub fn temp_path(case_name: &str, suffix: &str) -> PathBuf {
//     let mut builder = tempfile::Builder::new();
//     let prefix = ["ckb-it", case_name, suffix, ""].join("-");
//     builder.prefix(&prefix);
//     let tempdir = if let Ok(val) = env::var("CKB_INTEGRATION_TEST_TMP") {
//         builder.tempdir_in(val)
//     } else {
//         builder.tempdir()
//     }
//     .expect("create tempdir failed");
//     let path = tempdir.path().to_owned();
//     tempdir.close().expect("close tempdir failed");
//     path
// }

// /// Generate new blocks and explode these cellbases into `n` live cells
// pub fn generate_utxo_set(node: &Node, n: usize) -> TXOSet {
//     // Ensure all the cellbases will be used later are already mature.
//     let cellbase_maturity = node.consensus().cellbase_maturity();
//     mine(node, cellbase_maturity.index());
//
//     // Explode these mature cellbases into multiple cells
//     let mut n_outputs = 0;
//     let mut txs = Vec::new();
//     while n > n_outputs {
//         mine(node, 1);
//         let mature_number = node.get_tip_block_number() - cellbase_maturity.index();
//         let mature_block = node.get_block_by_number(mature_number);
//         let mature_cellbase = mature_block.transaction(0).unwrap();
//         if mature_cellbase.outputs().is_empty() {
//             continue;
//         }
//
//         let mature_utxos: TXOSet = TXOSet::from(&mature_cellbase);
//         let tx = mature_utxos.boom(vec![node.always_success_cell_dep()]);
//         n_outputs += tx.outputs().len();
//         txs.push(tx);
//     }
//
//     // Ensure all the transactions were committed
//     txs.iter().for_each(|tx| {
//         node.submit_transaction(tx);
//     });
//     while txs.iter().any(|tx| !is_transaction_committed(node, tx)) {
//         mine(node, node.consensus().finalization_delay_length());
//     }
//
//     let mut utxos = TXOSet::default();
//     txs.iter()
//         .for_each(|tx| utxos.extend(Into::<TXOSet>::into(tx)));
//     utxos.truncate(n);
//     utxos
// }

/// Return a blank block with additional committed transactions
pub fn commit(node: &Node, committed: &[&TransactionView]) -> BlockView {
    let committed = committed
        .iter()
        .map(|t| t.to_owned().to_owned())
        .collect::<Vec<_>>();
    blank(node)
        .as_advanced_builder()
        .transactions(committed)
        .build()
}

/// Return a blank block with additional proposed transactions
pub fn propose(node: &Node, proposals: &[&TransactionView]) -> BlockView {
    let proposals = proposals.iter().map(|tx| tx.proposal_short_id());
    blank(node)
        .as_advanced_builder()
        .proposals(proposals)
        .build()
}

/// Return a block with `proposals = [], transactions = [cellbase], uncles = []`
pub fn blank(node: &Node) -> BlockView {
    let example = node.new_block(None, None, None);
    example
        .as_advanced_builder()
        .set_proposals(vec![])
        .set_transactions(vec![example.transaction(0).unwrap()]) // cellbase
        .set_uncles(vec![])
        .build()
}

// grep "panicked at" $node_log_path
pub fn nodes_panicked(node_log_paths: &[PathBuf]) -> bool {
    node_log_paths.iter().any(|log_path| {
        read_to_string(log_path)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to read node's log {}, error: {:?}",
                    log_path.display(),
                    err
                )
            })
            .contains("panicked at")
    })
}

pub fn now_ms() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_millis() as u64
}

pub fn find_available_port() -> u16 {
    for _ in 0..2000 {
        let port = PORT_COUNTER.fetch_add(1, SeqCst);
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        if TcpListener::bind(address).is_ok() {
            return port;
        }
    }
    panic!("failed to allocate available port")
}

pub fn message_name(data: &Bytes) -> String {
    if let Ok(message) = SyncMessage::from_slice(data) {
        message.to_enum().item_name().to_string()
    } else if let Ok(message) = RelayMessage::from_slice(data) {
        message.to_enum().item_name().to_string()
    } else {
        panic!("unknown message item");
    }
}

pub fn is_transaction_pending(node: &Node, transaction: &TransactionView) -> bool {
    node.rpc_client()
        .get_transaction(transaction.hash())
        .map(|txstatus| txstatus.tx_status.status == Status::Pending)
        .unwrap_or(false)
}

pub fn is_transaction_proposed(node: &Node, transaction: &TransactionView) -> bool {
    node.rpc_client()
        .get_transaction(transaction.hash())
        .map(|txstatus| txstatus.tx_status.status == Status::Proposed)
        .unwrap_or(false)
}

pub fn is_transaction_committed(node: &Node, transaction: &TransactionView) -> bool {
    node.rpc_client()
        .get_transaction(transaction.hash())
        .map(|txstatus| txstatus.tx_status.status == Status::Committed)
        .unwrap_or(false)
}

pub fn is_transaction_unknown(node: &Node, transaction: &TransactionView) -> bool {
    node.rpc_client()
        .get_transaction(transaction.hash())
        .is_none()
}

pub fn assert_epoch_should_be(node: &Node, number: u64, index: u64, length: u64) {
    let tip_header: HeaderView = node.rpc_client().get_tip_header().into();
    let tip_epoch = tip_header.epoch();
    let target_epoch = EpochNumberWithFraction::new(number, index, length);
    assert_eq!(
        tip_epoch, target_epoch,
        "current tip epoch is {}, but expect epoch {}",
        tip_epoch, target_epoch
    );
}

pub fn assert_epoch_should_less_than(node: &Node, number: u64, index: u64, length: u64) {
    let tip_header: HeaderView = node.rpc_client().get_tip_header().into();
    let tip_epoch = tip_header.epoch();
    let target_epoch = EpochNumberWithFraction::new(number, index, length);
    assert!(
        tip_epoch < target_epoch,
        "current tip epoch is {}, but expect epoch less than {}",
        tip_epoch,
        target_epoch
    );
}

pub fn assert_epoch_should_greater_than(node: &Node, number: u64, index: u64, length: u64) {
    let tip_header: HeaderView = node.rpc_client().get_tip_header().into();
    let tip_epoch = tip_header.epoch();
    let target_epoch = EpochNumberWithFraction::new(number, index, length);
    assert!(
        tip_epoch > target_epoch,
        "current tip epoch is {}, but expect epoch greater than {}",
        tip_epoch,
        target_epoch
    );
}

pub fn assert_submit_block_fail(node: &Node, block: &BlockView, message: &str) {
    let result = node
        .rpc_client()
        .submit_block("".to_owned(), block.data().into());
    assert!(
        result.is_err(),
        "expect error \"{}\" but got \"Ok(())\"",
        message,
    );
    let error = result.expect_err(&format!("block is invalid since {}", message));
    let error_string = error.to_string();
    assert!(
        error_string.contains(message),
        "expect error \"{}\" but got \"{}\"",
        message,
        error_string,
    );
}

pub fn assert_submit_block_ok(node: &Node, block: &BlockView) {
    let result = node
        .rpc_client()
        .submit_block("".to_owned(), block.data().into());
    assert!(result.is_ok(), "expect \"Ok(())\" but got \"{:?}\"", result,);
}