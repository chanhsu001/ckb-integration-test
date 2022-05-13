use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;

use ckb_chain_spec::consensus::TYPE_ID_CODE_HASH;
use ckb_crypto::secp::Privkey;
use ckb_hash::{blake2b_256, new_blake2b};
use ckb_system_scripts::BUNDLED_CELL;
use ckb_types::core::DepType;
use ckb_types::{
    bytes::Bytes,
    core::{BlockView, Capacity, ScriptHashType, TransactionView},
    packed,
    packed::{CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
    prelude::*,
    H256,
};
use lazy_static::lazy_static;

use crate::node::Node;

pub mod mining;
pub mod node;
pub mod rpc;
pub mod utils;

// const MIN_FEE_RATE: u64 = 1_000;
// disable FEE_RATE for simplification
pub const MIN_FEE_RATE: u64 = 0;
pub const MIN_CELL_CAP: u64 = 9_000_000_000;

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
pub(crate) fn build_type_id_script(input: &packed::CellInput, output_index: u64) -> Script {
    let mut blake2b = new_blake2b();
    blake2b.update(input.as_slice());
    blake2b.update(&output_index.to_le_bytes());
    let mut ret = [0; 32];
    blake2b.finalize(&mut ret);
    let script_arg = Bytes::from(ret.to_vec());
    Script::new_builder()
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
    pub private_key: Privkey,
    pub lock_args: Script,
    pub bytes_lock_args: Bytes,
    pub cell_cap: u64,
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
        let next_privkey = blake2b_256(template).pack();
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

/// save account cellcap info, in case of pause and re-run
pub fn save_accounts_cellcap_to_file(accounts: &[Account], file: &PathBuf) {
    let accounts_cell_cap = accounts
        .iter()
        .map(|account| account.cell_cap)
        .collect::<Vec<u64>>();
    let content = serde_json::to_string(&accounts_cell_cap).expect("serialize account cell cap");
    let mut save = OpenOptions::new()
        .write(true)
        .create(true)
        .open(file)
        .expect("load account cell cap file error");
    save.write_all(content.as_ref()).expect("write_all error?");
}

/// load account cellcap from file to recovery accounts
/// accounts key info is same whenever accounts recreated
pub fn load_accounts_from_file(accounts: &mut [Account], file: &PathBuf) {
    let mut f = OpenOptions::new()
        .read(true)
        .open(file)
        .expect("load account cell cap file error");
    let mut cap_data = String::new();
    f.read_to_string(&mut cap_data)
        .expect("cell data read error");
    let cellcap: Vec<u64> =
        serde_json::from_str(cap_data.as_str()).expect("Deserialised from account_cellcap.dat");
    assert_eq!(cellcap.len(), accounts.len());
    for (account, cellcap) in accounts.iter_mut().zip(cellcap.iter()) {
        account.cell_cap = *cellcap;
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
pub fn attach_witness(mut tx: TransactionView, signed_accounts: &[Account]) -> TransactionView {
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

#[inline]
pub fn parent_block_2tx_1output_as_new_input(node: &Node) -> CellInput {
    let parent = node.get_tip_block();
    output_as_new_input(&parent, 1, 0)
}

#[inline]
pub fn genesis_block_1tx_8output_as_new_input(node: &Node) -> CellInput {
    let genesis_block = node.get_block_by_number(0);
    output_as_new_input(&genesis_block, 0, 7)
}

#[inline]
fn output_as_new_input(parent: &BlockView, tx_index: usize, output_index: u32) -> CellInput {
    let txs = parent.transactions();
    let tx = txs.get(tx_index).expect("get live_cell transaction");
    CellInput::new(OutPoint::new(tx.hash(), output_index), parent.number())
}
