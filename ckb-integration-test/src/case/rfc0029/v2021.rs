use super::util::Deployer;
use super::{ERROR_DUPLICATE_CELL_DEPS, ERROR_MULTIPLE_MATCHES, PASS};
use crate::case::rfc0029::{is_rfc0029_switched, RFC0029_EPOCH_NUMBER};
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::CKB2021;
use ckb_testkit::{BuildInstruction, NodeOptions, Nodes};
use ckb_types::core::{Capacity, DepType, TransactionBuilder};
use ckb_types::packed::{CellDep, CellInput, OutPointVec};
use ckb_types::{
    core::ScriptHashType,
    packed::{CellOutput, Script},
    prelude::*,
};

pub struct RFC0029V2021;

impl Case for RFC0029V2021 {
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
        {
            let rfc0029_switch = calc_epoch_start_number(node2021, RFC0029_EPOCH_NUMBER);
            let current_tip_number = node2021.get_tip_block_number();
            if rfc0029_switch > current_tip_number {
                node2021.mine(rfc0029_switch - current_tip_number + 1);
            }
        }

        let mut deployer = Deployer::default();
        let type_script = node2021
            .always_success_script()
            .as_builder()
            .args("no-matter".pack())
            .build();

        // deploy "a1"
        {
            let output_data =
                include_bytes!("../../../testdata/spec/ckb2021/cells/always_success").pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .type_(Some(type_script.clone()).pack())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "a1", output, output_data)
        }
        // deploy "a2"
        {
            let output_data =
                include_bytes!("../../../testdata/spec/ckb2021/cells/always_success").pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .type_(Some(type_script.clone()).pack())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "a2", output, output_data)
        }
        // deploy "b1"
        {
            let output_data =
                include_bytes!("../../../testdata/spec/ckb2021/cells/another_always_success")
                    .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .type_(Some(type_script.clone()).pack())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "b1", output, output_data)
        }
        // deploy Group("a1"), naming "group_a1"
        {
            let output_data = OutPointVec::new_builder()
                .set(vec![deployer.get_out_point("a1")])
                .build()
                .as_bytes()
                .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "group_a1", output, output_data)
        }
        // deploy Group("a2"), naming "group_a2"
        {
            let output_data = OutPointVec::new_builder()
                .set(vec![deployer.get_out_point("a2")])
                .build()
                .as_bytes()
                .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "group_a2", output, output_data)
        }
        // deploy Group("b1"), naming "group_b1"
        {
            let output_data = OutPointVec::new_builder()
                .set(vec![deployer.get_out_point("b1")])
                .build()
                .as_bytes()
                .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "group_b1", output, output_data)
        }
        // deploy Group("a1", "a1"), naming "group_a1_a1"
        {
            let output_data = OutPointVec::new_builder()
                .set(vec![
                    deployer.get_out_point("a1"),
                    deployer.get_out_point("a1"),
                ])
                .build()
                .as_bytes()
                .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "group_a1_a1", output, output_data)
        }
        // deploy Group("a1", "a2"), naming "group_a1_a2"
        {
            let output_data = OutPointVec::new_builder()
                .set(vec![
                    deployer.get_out_point("a1"),
                    deployer.get_out_point("a2"),
                ])
                .build()
                .as_bytes()
                .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "group_a1_a2", output, output_data)
        }
        // deploy Group("a1", "b1"), naming "group_a1_b1"
        {
            let output_data = OutPointVec::new_builder()
                .set(vec![
                    deployer.get_out_point("a1"),
                    deployer.get_out_point("b1"),
                ])
                .build()
                .as_bytes()
                .pack();
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .unwrap();
            deployer.deploy(node2021, "group_a1_b1", output, output_data)
        }

        // Make sure we are after rfc0029 switch
        assert!(is_rfc0029_switched(node2021));

        let code_hash_via_data_hash = {
            let out_point = deployer.get_out_point("a1");
            let cell_with_status = node2021.rpc_client().get_live_cell(out_point.into(), true);
            let raw_data = cell_with_status.cell.unwrap().data.unwrap().content;
            CellOutput::calc_data_hash(raw_data.as_bytes())
        };
        let code_hash_via_type_hash = { type_script.calc_script_hash() };

        let cases = vec![
            // (script.hash_type, cell_deps, expected_result)
            (ScriptHashType::Data, vec!["a1"], PASS),
            (
                ScriptHashType::Data,
                vec!["a1", "a1"],
                ERROR_DUPLICATE_CELL_DEPS,
            ),
            (ScriptHashType::Data, vec!["a1", "a2"], PASS),
            (ScriptHashType::Data, vec!["a1", "b1"], PASS),
            (ScriptHashType::Data, vec!["group_a1"], PASS),
            (ScriptHashType::Data, vec!["group_a1_a1"], PASS),
            (ScriptHashType::Data, vec!["group_a1_a2"], PASS),
            (ScriptHashType::Data, vec!["group_a1_b1"], PASS),
            (ScriptHashType::Data, vec!["group_a1", "a1"], PASS),
            (ScriptHashType::Data, vec!["group_a1", "a2"], PASS),
            (ScriptHashType::Data, vec!["group_a1", "b1"], PASS),
            (ScriptHashType::Data, vec!["group_a1", "group_a2"], PASS),
            (ScriptHashType::Data, vec!["group_a1", "group_b1"], PASS),
            (ScriptHashType::Type, vec!["a1"], PASS),
            (
                ScriptHashType::Type,
                vec!["a1", "a1"],
                ERROR_DUPLICATE_CELL_DEPS,
            ),
            (ScriptHashType::Type, vec!["a1", "a2"], PASS),
            (
                ScriptHashType::Type,
                vec!["a1", "b1"],
                ERROR_MULTIPLE_MATCHES,
            ),
            (ScriptHashType::Type, vec!["group_a1"], PASS),
            (ScriptHashType::Type, vec!["group_a1_a1"], PASS),
            (ScriptHashType::Type, vec!["group_a1_a2"], PASS),
            (
                ScriptHashType::Type,
                vec!["group_a1_b1"],
                ERROR_MULTIPLE_MATCHES,
            ),
            (ScriptHashType::Type, vec!["group_a1", "a1"], PASS),
            (ScriptHashType::Type, vec!["group_a1", "a2"], PASS),
            (
                ScriptHashType::Type,
                vec!["group_a1", "b1"],
                ERROR_MULTIPLE_MATCHES,
            ),
            (ScriptHashType::Type, vec!["group_a1", "group_a2"], PASS),
            (
                ScriptHashType::Type,
                vec!["group_a1", "group_b1"],
                ERROR_MULTIPLE_MATCHES,
            ),
        ];
        let inputs = node2021.get_spendable_always_success_cells();

        for (i, (script_hash_type, str_cell_deps, expected_result)) in cases.into_iter().enumerate()
        {
            ckb_testkit::info!(
                "case-{:02} script_hash_type: {}, cell_deps: {:?}, expected_result: \"{}\"",
                i,
                script_hash_type as u8,
                str_cell_deps,
                expected_result
            );
            let input = &inputs[i];
            let type_ = Script::new_builder()
                .hash_type(script_hash_type.into())
                .code_hash({
                    match script_hash_type {
                        ScriptHashType::Data => code_hash_via_data_hash.clone(),
                        ScriptHashType::Type => code_hash_via_type_hash.clone(),
                        ScriptHashType::Data1 => unreachable!(),
                    }
                })
                .build();
            let output = CellOutput::new_builder()
                .type_(Some(type_).pack())
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::zero())
                .unwrap();
            let mut cell_deps = str_cell_deps
                .into_iter()
                .map(|cell_name| {
                    let cell_meta = deployer.get_cell_meta(cell_name);
                    if cell_name.contains("group") {
                        CellDep::new_builder()
                            .dep_type(DepType::DepGroup.into())
                            .out_point(cell_meta.out_point)
                            .build()
                    } else {
                        CellDep::new_builder()
                            .dep_type(DepType::Code.into())
                            .out_point(cell_meta.out_point)
                            .build()
                    }
                })
                .collect::<Vec<_>>();
            // Lock script need always_success_cell_dep
            cell_deps.push(node2021.always_success_cell_dep());

            let tx = TransactionBuilder::default()
                .input(CellInput::new(input.out_point.clone(), 0))
                .output(output)
                .output_data(Default::default())
                .cell_deps(cell_deps)
                .build();

            if expected_result == PASS {
                let tip_number = node2021.get_tip_block_number();
                let instructions = vec![
                    BuildInstruction::SendTransaction {
                        template_number: tip_number + 1,
                        transaction: tx.clone(),
                    },
                    BuildInstruction::Propose {
                        template_number: tip_number + 1,
                        proposal_short_id: tx.proposal_short_id(),
                    },
                    BuildInstruction::Commit {
                        template_number: tip_number + 3,
                        transaction: tx.clone(),
                    },
                ];

                node2021
                    .build_according_to_instructions(tip_number + 3, instructions.clone())
                    .unwrap_or_else(|err| {
                        panic!(
                            "case-{} failed on {}, error: {}",
                            i,
                            node2021.node_name(),
                            err
                        )
                    });
            } else {
                let result1 = node2021
                    .rpc_client()
                    .send_transaction_result(tx.data().into());
                assert!(
                    result1.is_err(),
                    "for case-{}, expect node-{} returning error \"{}\" for tx {:#x}, but got ok",
                    i,
                    node2021.node_name(),
                    expected_result,
                    tx.hash(),
                );
            }
        }
    }
}
