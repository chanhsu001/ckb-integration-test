use super::{
    ERROR_INVALID_ECALL, PLACE_CELL_DATA, PLACE_WITNESS, RFC0034_EPOCH_NUMBER, SOURCE_DEP,
    SOURCE_INPUT, SOURCE_OUTPUT,
};
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::util::deployer::Deployer;
use crate::util::instructions::{
    instructions_of_failed_to_commit_transaction_after_switch,
    instructions_of_failed_to_commit_transaction_before_switch,
    instructions_of_failed_to_send_transaction_after_switch,
    instructions_of_failed_to_send_transaction_before_switch,
    instructions_of_success_to_send_transaction_after_switch,
    instructions_of_success_to_send_transaction_before_switch,
};
use crate::CKB2021;
use ckb_exec_params::ExecParams;
use ckb_testkit::{assert_result_eq, Node, NodeOptions, Nodes};
use ckb_types::core::Capacity;
use ckb_types::{
    core::{ScriptHashType, TransactionBuilder, TransactionView},
    packed::{Bytes, CellDep, CellInput, CellOutput, OutPoint, Script},
    prelude::*,
};

pub struct RFC0034;

impl Case for RFC0034 {
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

        let mut deployer = Deployer::new();
        {
            let scripts_data = vec![
                (
                    "exec_callee",
                    // include_bytes!("../../../testdata/script/exec_callee").pack(),
                    include_bytes!("/Users/keroro520/cryptape/exec_test_script/target/riscv64imac-unknown-none-elf/release/exec_callee").pack()
                ),
                (
                    "exec_caller",
                    // include_bytes!("../../../testdata/script/exec_caller").pack(),
                    include_bytes!("/Users/keroro520/cryptape/exec_test_script/target/riscv64imac-unknown-none-elf/release/exec_caller").pack()
                ),
            ];
            for (script_name, script_data) in scripts_data {
                // We construct type_script of deployed cells, so that we can depend them via
                // `script.hash_type == ScriptHashType::Type`.
                let deployed_type_script = node2021
                    .always_success_script()
                    .as_builder()
                    .hash_type(ScriptHashType::Type.into())
                    .args(script_name.pack())
                    .build();
                let output = CellOutput::new_builder()
                    .lock(node2021.always_success_script())
                    .type_(Some(deployed_type_script.clone()).pack())
                    .build_exact_capacity(Capacity::bytes(script_data.len()).unwrap())
                    .unwrap();
                deployer.deploy(node2021, script_name, output, script_data);
            }
        }

        // node2021 moves around fork switch height
        let fork_switch_height = calc_epoch_start_number(node2021, RFC0034_EPOCH_NUMBER);
        node2021.mine_to(fork_switch_height - 10);

        // [(case_id, source, place, expected_result_before_switch, expected_result_after_switch)])]
        let cases = vec![
            (
                0,
                SOURCE_OUTPUT,
                PLACE_CELL_DATA,
                Err(ERROR_INVALID_ECALL),
                Ok(()),
            ),
            (
                1,
                SOURCE_OUTPUT,
                PLACE_WITNESS,
                Err(ERROR_INVALID_ECALL),
                Ok(()),
            ),
            (
                2,
                SOURCE_INPUT,
                PLACE_CELL_DATA,
                Err(ERROR_INVALID_ECALL),
                Ok(()),
            ),
            (
                3,
                SOURCE_INPUT,
                PLACE_WITNESS,
                Err(ERROR_INVALID_ECALL),
                Ok(()),
            ),
            (
                4,
                SOURCE_DEP,
                PLACE_CELL_DATA,
                Err(ERROR_INVALID_ECALL),
                Ok(()),
            ),
            (
                5,
                SOURCE_DEP,
                PLACE_WITNESS,
                Err(ERROR_INVALID_ECALL),
                Err("TransactionScriptError"),
            ),
        ];
        for (case_id, source, place, expected_result_before_switch, expected_result_after_switch) in
            cases
        {
            let txs = build_transactions(node2021, &deployer, source, place);

            {
                let node = node2021.clone_node(&format!("case-{}-node2021-before-switch", case_id));
                run_case_before_switch(&node, case_id, txs.clone(), expected_result_before_switch);
            }

            {
                let node = node2021.clone_node(&format!("case-{}-node2021-after-switch", case_id));
                run_case_after_switch(&node, case_id, txs, expected_result_after_switch);
            }
        }
    }
}

fn run_case_before_switch(
    node: &Node,
    case_id: usize,
    txs: Vec<TransactionView>,
    expected_result_before_switch: Result<(), &str>,
) {
    let fork_switch_height = calc_epoch_start_number(node, RFC0034_EPOCH_NUMBER);
    assert!(node.get_tip_block_number() <= fork_switch_height - 4);

    if expected_result_before_switch.is_ok() {
        let instructions = txs.iter().fold(vec![], |mut acc, tx| {
            let is =
                instructions_of_success_to_send_transaction_before_switch(fork_switch_height, tx);
            acc.extend(is);
            acc
        });
        let actual_result_before_switch =
            node.build_according_to_instructions(fork_switch_height, instructions);
        assert_result_eq!(
            expected_result_before_switch,
            actual_result_before_switch,
            "\ncase-{} expected_result_before_switch: {:?}, actual_result_before_switch: {:?}",
            case_id,
            expected_result_before_switch,
            actual_result_before_switch,
        );
    } else {
        // test sending transaction
        {
            let instructions = txs.iter().fold(vec![], |mut acc, tx| {
                let is = instructions_of_failed_to_send_transaction_before_switch(
                    fork_switch_height,
                    tx,
                );
                acc.extend(is);
                acc
            });
            let actual_result_before_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_before_switch,
                actual_result_before_switch,
                "\ncase-{} expected_result_before_switch: {:?}, actual_result_before_switch: {:?}",
                case_id,
                expected_result_before_switch,
                actual_result_before_switch
            );
        }

        // test committing transaction
        {
            let instructions = txs.iter().fold(vec![], |mut acc, tx| {
                let is = instructions_of_failed_to_commit_transaction_before_switch(
                    fork_switch_height,
                    tx,
                );
                acc.extend(is);
                acc
            });
            let actual_result_before_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_before_switch,
                actual_result_before_switch,
                "\ncase-{} expected_result_before_switch: {:?}, actual_result_before_switch: {:?}",
                case_id,
                expected_result_before_switch,
                actual_result_before_switch
            );
        }
    }
}

fn run_case_after_switch(
    node: &Node,
    case_id: usize,
    txs: Vec<TransactionView>,
    expected_result_after_switch: Result<(), &str>,
) {
    let fork_switch_height = calc_epoch_start_number(node, RFC0034_EPOCH_NUMBER);
    assert!(node.get_tip_block_number() <= fork_switch_height - 4);

    if expected_result_after_switch.is_ok() {
        let instructions = txs.iter().fold(vec![], |mut acc, tx| {
            let is =
                instructions_of_success_to_send_transaction_after_switch(fork_switch_height, tx);
            acc.extend(is);
            acc
        });
        let actual_result_after_switch =
            node.build_according_to_instructions(fork_switch_height, instructions);
        assert_result_eq!(
            expected_result_after_switch,
            actual_result_after_switch,
            "\ncase-{} expected_result_after_switch: {:?}, actual_result_after_switch: {:?}",
            case_id,
            expected_result_after_switch,
            actual_result_after_switch,
        );
    } else {
        // test sending transaction
        {
            let instructions = txs.iter().fold(vec![], |mut acc, tx| {
                let is =
                    instructions_of_failed_to_send_transaction_after_switch(fork_switch_height, tx);
                acc.extend(is);
                acc
            });
            let actual_result_after_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_after_switch,
                actual_result_after_switch,
                "\ncase-{} expected_result_after_switch: {:?}, actual_result_after_switch: {:?}",
                case_id,
                expected_result_after_switch,
                actual_result_after_switch,
            );
        }

        // test committing transaction
        {
            let instructions = txs.iter().fold(vec![], |mut acc, tx| {
                let is = instructions_of_failed_to_commit_transaction_after_switch(
                    fork_switch_height,
                    tx,
                );
                acc.extend(is);
                acc
            });
            let actual_result_after_switch =
                node.build_according_to_instructions(fork_switch_height, instructions);
            assert_result_eq!(
                expected_result_after_switch,
                actual_result_after_switch,
                "\ncase-{} expected_result_after_switch: {:?}, actual_result_after_switch: {:?}",
                case_id,
                expected_result_after_switch,
                actual_result_after_switch,
            );
        }
    }
}

fn build_transactions(
    node: &Node,
    deployer: &Deployer,
    source: u32,
    place: u32,
) -> Vec<TransactionView> {
    let mut spendable = node.get_spendable_always_success_cells();

    // Prepare common-used utils
    let exec_callee_data: Bytes = {
        let exec_callee_cell = deployer.get_cell("exec_callee");
        let cell_with_status = node
            .rpc_client()
            .get_live_cell(exec_callee_cell.out_point.clone().into(), true);
        let raw_data = cell_with_status.cell.unwrap().data.unwrap().content;
        raw_data.into_bytes().pack()
    };
    let exec_caller_cell_dep = CellDep::new_builder()
        .out_point(deployer.get_cell("exec_caller").out_point.clone())
        .build();
    let exec_caller_output = {
        let exec_params = ExecParams::new_builder()
            .source(ckb_exec_params::ckb_types::prelude::Pack::pack(&source))
            .place(ckb_exec_params::ckb_types::prelude::Pack::pack(&place))
            .index(ckb_exec_params::ckb_types::prelude::Pack::pack(&0u32))
            .bounds(ckb_exec_params::ckb_types::prelude::Pack::pack(&0u64))
            // .expected_result(Default::default())
            // .expected_result(Default::default())
            .build();
        let exec_caller_type_hash = deployer
            .get_cell("exec_caller")
            .cell_output
            .type_()
            .to_opt()
            .unwrap()
            .calc_script_hash();
        let output_type_script = Script::new_builder()
            .hash_type(ScriptHashType::Type.into())
            .code_hash(exec_caller_type_hash)
            // `exec_params.as_slice().pack()` 和 `exec_params.as_bytes().pack()` 有什么区别
            .args(exec_params.as_slice().pack())
            // .args(exec_params.as_reader())
            .build();
        CellOutput::new_builder()
            .lock(node.always_success_script())
            .type_(Some(output_type_script).pack())
            .build_exact_capacity(Capacity::bytes(exec_callee_data.len()).unwrap())
            .unwrap()
    };
    let dep_tx = {
        let inputs = spendable.split_off(spendable.len() - 100);
        let capacity: u64 = inputs.iter().map(|input| input.capacity().as_u64()).sum();
        TransactionBuilder::default()
            .inputs(
                inputs
                    .iter()
                    .map(|input| CellInput::new(input.out_point.clone(), 0)),
            )
            .output(
                CellOutput::new_builder()
                    .lock(inputs[0].cell_output.lock())
                    .type_(inputs[0].cell_output.type_())
                    .capacity(capacity.pack())
                    .build(),
            )
            .output_data(exec_callee_data.clone())
            .cell_dep(node.always_success_cell_dep())
            .build()
    };

    if source == SOURCE_INPUT && place == PLACE_CELL_DATA {
        let tx = TransactionBuilder::default()
            .input(CellInput::new(OutPoint::new(dep_tx.hash(), 0), 0))
            .output(exec_caller_output)
            .output_data(Default::default())
            .cell_dep(exec_caller_cell_dep.clone())
            .cell_dep(node.always_success_cell_dep())
            .build();
        return vec![dep_tx, tx];
    }
    if source == SOURCE_DEP && place == PLACE_CELL_DATA {
        let inputs = spendable.split_off(spendable.len() - 100);
        let tx = TransactionBuilder::default()
            .inputs(
                inputs
                    .into_iter()
                    .map(|input| CellInput::new(input.out_point, 0)),
            )
            .output(exec_caller_output)
            .output_data(Default::default())
            .cell_dep(
                CellDep::new_builder()
                    .out_point(OutPoint::new(dep_tx.hash(), 0))
                    .build(),
            )
            .cell_dep(exec_caller_cell_dep.clone())
            .cell_dep(node.always_success_cell_dep())
            .build();
        return vec![dep_tx, tx];
    }

    let mut tx_builder = TransactionBuilder::default();

    // build tx's input
    // build tx's output
    // build tx's cell-dep
    let inputs = spendable.split_off(spendable.len() - 100);
    tx_builder = tx_builder
        .inputs(
            inputs
                .into_iter()
                .map(|input| CellInput::new(input.out_point, 0)),
        )
        .output(exec_caller_output)
        .cell_dep(exec_caller_cell_dep.clone())
        .cell_dep(node.always_success_cell_dep());

    // build tx's output-data
    if place == PLACE_CELL_DATA {
        tx_builder = tx_builder.output_data(exec_callee_data.clone());
    } else {
        tx_builder = tx_builder.output_data(Default::default())
    }

    // build tx's witness
    if place == PLACE_WITNESS {
        tx_builder = tx_builder.witness(exec_callee_data);
    }

    let tx = tx_builder.build();
    vec![dep_tx, tx]
}
