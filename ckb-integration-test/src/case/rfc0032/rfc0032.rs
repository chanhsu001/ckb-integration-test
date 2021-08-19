use super::{ERROR_INCOMPATIBLE, ERROR_INVALID_VM_VERSION, RFC0032_EPOCH_NUMBER};
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::util::instructions::{
    instructions_of_failed_to_commit_transaction_after_switch,
    instructions_of_failed_to_commit_transaction_before_switch,
    instructions_of_failed_to_send_transaction_after_switch,
    instructions_of_failed_to_send_transaction_before_switch,
    instructions_of_success_to_send_transaction_after_switch,
    instructions_of_success_to_send_transaction_before_switch,
};
use crate::CKB2021;
use ckb_crypto::secp::Privkey;
use ckb_jsonrpc_types::CellInfo;
use ckb_testkit::{
    assert_result_eq, Node, NodeOptions, Nodes, User, SIGHASH_ALL_DATA_HASH, SIGHASH_ALL_TYPE_HASH,
    SYSTEM_CELL_ALWAYS_SUCCESS_INDEX,
};
use ckb_types::{
    core::{ScriptHashType, TransactionBuilder, TransactionView},
    packed::{CellInput, CellOutput, OutPoint, Script},
    prelude::*,
};

pub struct RFC0032;

impl Case for RFC0032 {
    fn case_options(&self) -> CaseOptions {
        CaseOptions {
            make_all_nodes_connected: false,
            make_all_nodes_synced: false,
            make_all_nodes_connected_and_synced: false,
            node_options: vec![NodeOptions {
                node_name: String::from("node2021"),
                ckb_binary: CKB2021.read().unwrap().clone(),
                initial_database: "testdata/db/Epoch2V2TestData",
                chain_spec: "testdata/spec/ckb2021",
                app_config: "testdata/config/ckb2021",
            }],
        }
    }

    fn run(&self, nodes: Nodes) {
        let node2021 = nodes.get_node("node2021");
        let user = {
            let genesis_block = node2021.get_block_by_number(0);
            let random_str = node2021.get_tip_block().hash();
            let privkey = Privkey::from_slice(random_str.as_slice());
            User::new(genesis_block, Some(privkey))
        };

        // node2021 moves around fork switch height
        let fork_switch_height = calc_epoch_start_number(node2021, RFC0032_EPOCH_NUMBER);
        node2021.mine_to(fork_switch_height - 10);

        // [(case_id, lock.hash_type, type.hash_type, expected_result_before_switch, expected_result_after_switch)]
        let cases: Vec<(
            usize,
            ScriptHashType,
            Option<ScriptHashType>,
            Result<(), &str>,
            Result<(), &str>,
        )> = vec![
            (0, ScriptHashType::Data, None, Ok(()), Ok(())),
            (1, ScriptHashType::Type, None, Ok(()), Ok(())),
            (
                2,
                ScriptHashType::Data1,
                None,
                Err(ERROR_INCOMPATIBLE),
                Ok(()),
            ),
            (
                3,
                ScriptHashType::Data,
                Some(ScriptHashType::Data),
                Ok(()),
                Ok(()),
            ),
            (
                4,
                ScriptHashType::Type,
                Some(ScriptHashType::Data),
                Ok(()),
                Ok(()),
            ),
            (
                5,
                ScriptHashType::Data1,
                Some(ScriptHashType::Data),
                Err(ERROR_INCOMPATIBLE),
                Ok(()),
            ),
            (
                6,
                ScriptHashType::Data,
                Some(ScriptHashType::Type),
                Ok(()),
                Ok(()),
            ),
            (
                7,
                ScriptHashType::Type,
                Some(ScriptHashType::Type),
                Ok(()),
                Ok(()),
            ),
            (
                8,
                ScriptHashType::Data1,
                Some(ScriptHashType::Type),
                Err(ERROR_INCOMPATIBLE),
                Ok(()),
            ),
            (
                9,
                ScriptHashType::Data,
                Some(ScriptHashType::Data1),
                Err(ERROR_INVALID_VM_VERSION),
                Ok(()),
            ),
            (
                10,
                ScriptHashType::Type,
                Some(ScriptHashType::Data1),
                Err(ERROR_INVALID_VM_VERSION),
                Ok(()),
            ),
            (
                11,
                ScriptHashType::Data1,
                Some(ScriptHashType::Data1),
                Err(ERROR_INCOMPATIBLE),
                Ok(()),
            ),
        ];
        for (
            case_id,
            lock_script_hash_type,
            type_script_hash_type,
            expected_result_before_switch,
            expected_result_after_switch,
        ) in cases
        {
            {
                let node = node2021.clone_node(&format!("case-{}-node2021-before-switch", case_id));
                run_case_before_switch(
                    &node,
                    &user,
                    case_id,
                    lock_script_hash_type,
                    type_script_hash_type,
                    expected_result_before_switch,
                );
            }

            {
                let node = node2021.clone_node(&format!("case-{}-node2021-after-switch", case_id));
                run_case_after_switch(
                    &node,
                    &user,
                    case_id,
                    lock_script_hash_type,
                    type_script_hash_type,
                    expected_result_after_switch,
                );
            }
        }
    }
}

fn run_case_before_switch(
    node: &Node,
    user: &User,
    case_id: usize,
    lock_script_hash_type: ScriptHashType,
    type_script_hash_type: Option<ScriptHashType>,
    expected_result_before_switch: Result<(), &str>,
) {
    let fork_switch_height = calc_epoch_start_number(node, RFC0032_EPOCH_NUMBER);
    assert!(node.get_tip_block_number() <= fork_switch_height - 4);

    let transaction = build_transaction(node, &user, lock_script_hash_type, type_script_hash_type);
    if expected_result_before_switch.is_ok() {
        let instructions = instructions_of_success_to_send_transaction_before_switch(
            fork_switch_height,
            &transaction,
        );
        let actual_result_before_switch =
            node.build_according_to_instructions(fork_switch_height, instructions);
        assert_result_eq!(
            expected_result_before_switch,
            actual_result_before_switch,
            "case-{} expected_result_before_switch: {:?}, actual_result_before_switch: {:?}",
            case_id,
            expected_result_before_switch,
            actual_result_before_switch,
        );
    } else {
        // test sending transaction
        {
            let instructions = instructions_of_failed_to_send_transaction_before_switch(
                fork_switch_height,
                &transaction,
            );
            let actual_result_before_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_before_switch,
                actual_result_before_switch,
                "case-{} expected_result_before_switch: {:?}, actual_result_before_switch: {:?}",
                case_id,
                expected_result_before_switch,
                actual_result_before_switch,
            );
        }

        // test committing transaction
        {
            let instructions = instructions_of_failed_to_commit_transaction_before_switch(
                fork_switch_height,
                &transaction,
            );
            let actual_result_before_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_before_switch,
                actual_result_before_switch,
                "case-{} expected_result_before_switch: {:?}, actual_result_before_switch: {:?}",
                case_id,
                expected_result_before_switch,
                actual_result_before_switch
            );
        }
    }
}

fn run_case_after_switch(
    node: &Node,
    user: &User,
    case_id: usize,
    lock_script_hash_type: ScriptHashType,
    type_script_hash_type: Option<ScriptHashType>,
    expected_result_after_switch: Result<(), &str>,
) {
    let fork_switch_height = calc_epoch_start_number(node, RFC0032_EPOCH_NUMBER);
    assert!(node.get_tip_block_number() <= fork_switch_height - 4);

    let transaction = build_transaction(node, &user, lock_script_hash_type, type_script_hash_type);
    if expected_result_after_switch.is_ok() {
        let instructions = instructions_of_success_to_send_transaction_after_switch(
            fork_switch_height,
            &transaction,
        );
        let actual_result_after_switch =
            node.build_according_to_instructions(fork_switch_height, instructions);
        assert_result_eq!(
            expected_result_after_switch,
            actual_result_after_switch,
            "case-{} expected_result_after_switch: {:?}, actual_result_after_switch: {:?}",
            case_id,
            expected_result_after_switch,
            actual_result_after_switch,
        );
    } else {
        // test sending transaction
        {
            let instructions = instructions_of_failed_to_send_transaction_after_switch(
                fork_switch_height,
                &transaction,
            );
            let actual_result_after_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_after_switch,
                actual_result_after_switch,
                "case-{} expected_result_after_switch: {:?}, actual_result_after_switch: {:?}",
                case_id,
                expected_result_after_switch,
                actual_result_after_switch,
            );
        }

        // test committing transaction
        {
            let instructions = instructions_of_failed_to_commit_transaction_after_switch(
                fork_switch_height,
                &transaction,
            );
            let actual_result_after_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_after_switch,
                actual_result_after_switch,
                "case-{} expected_result_after_switch: {:?}, actual_result_after_switch: {:?}",
                case_id,
                expected_result_after_switch,
                actual_result_after_switch,
            );
        }
    }
}

fn build_transaction(
    node: &Node,
    user: &User,
    lock_script_hash_type: ScriptHashType,
    type_script_hash_type: Option<ScriptHashType>,
) -> TransactionView {
    let input = node
        .get_spendable_always_success_cells()
        .last()
        .unwrap()
        .to_owned();
    let lock = match lock_script_hash_type {
        ScriptHashType::Data => Script::new_builder()
            .hash_type(ScriptHashType::Data.into())
            .code_hash(SIGHASH_ALL_DATA_HASH.pack())
            .args(user.single_secp256k1_address().0.pack())
            .build(),
        ScriptHashType::Type => Script::new_builder()
            .hash_type(ScriptHashType::Type.into())
            .code_hash(SIGHASH_ALL_TYPE_HASH.pack())
            .args(user.single_secp256k1_address().0.pack())
            .build(),
        ScriptHashType::Data1 => Script::new_builder()
            .hash_type(ScriptHashType::Data1.into())
            .code_hash(SIGHASH_ALL_DATA_HASH.pack())
            .args(user.single_secp256k1_address().0.pack())
            .build(),
    };
    let type_: Option<Script> = if let Some(type_script_hash_type) = type_script_hash_type {
        let always_success_contract_cell_info: CellInfo = {
            let genesis_cellbase_hash = node.genesis_cellbase_hash();
            let always_success_out_point =
                OutPoint::new(genesis_cellbase_hash, SYSTEM_CELL_ALWAYS_SUCCESS_INDEX);
            let cell = node
                .rpc_client()
                .get_live_cell(always_success_out_point.into(), true);
            cell.cell.expect("genesis always cell must be live")
        };
        let always_success_contract_data = always_success_contract_cell_info
            .data
            .expect("get_live_cell with_data=true");
        let always_success_contract_output: CellOutput =
            always_success_contract_cell_info.output.into();
        let always_success_type_id_script = always_success_contract_output
            .type_()
            .to_opt()
            .expect("genesis always success cell should have type_=type-id script");
        match type_script_hash_type {
            ScriptHashType::Data => Some(
                Script::new_builder()
                    .code_hash(always_success_contract_data.hash.pack())
                    .hash_type(ScriptHashType::Data.into())
                    .build(),
            ),
            ScriptHashType::Type => Some(
                Script::new_builder()
                    .code_hash(always_success_type_id_script.calc_script_hash())
                    .hash_type(ScriptHashType::Type.into())
                    .build(),
            ),
            ScriptHashType::Data1 => Some(
                Script::new_builder()
                    .code_hash(always_success_contract_data.hash.pack())
                    .hash_type(ScriptHashType::Data1.into())
                    .build(),
            ),
        }
    } else {
        None
    };
    let unsigned_tx = TransactionBuilder::default()
        .input(CellInput::new(input.out_point.clone(), 0))
        .output(
            CellOutput::new_builder()
                .capacity(input.capacity().pack())
                .lock(lock)
                .type_(type_.pack())
                .build(),
        )
        .output_data(Default::default())
        .cell_dep(node.always_success_cell_dep())
        .cell_dep(user.single_secp256k1_cell_dep())
        .build();
    let witness = user.single_secp256k1_signed_witness(&unsigned_tx);
    unsigned_tx
        .as_advanced_builder()
        .witness(witness.as_bytes().pack())
        .build()
}
