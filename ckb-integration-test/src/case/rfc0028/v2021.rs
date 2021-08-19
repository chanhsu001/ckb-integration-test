use super::{ERROR_IMMATURE, PASS, RFC0028_EPOCH_NUMBER};
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::CKB2021;
use ckb_testkit::util::since_from_relative_timestamp;
use ckb_testkit::NodeOptions;
use ckb_testkit::{BuildInstruction, Nodes};
use ckb_types::core::Capacity;
use ckb_types::packed::OutPoint;
use ckb_types::{
    core::TransactionBuilder,
    packed::{CellInput, CellOutput},
    prelude::*,
};
use std::collections::HashSet;

pub struct RFC0028V2021;

// input_median_time 是从 input-block 开始算还是从 input-block.parent 开始算？
impl Case for RFC0028V2021 {
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
            }]
            .into_iter()
            .collect(),
        }
    }

    fn run(&self, nodes: Nodes) {
        let node2021 = nodes.get_node("node2021");
        let median_time_block_count = node2021.consensus().median_time_block_count.value();

        node2021.mine_to(
            calc_epoch_start_number(node2021, RFC0028_EPOCH_NUMBER) + median_time_block_count,
        );

        let t = node2021.get_tip_block().timestamp();
        let old_tip_number = node2021.get_tip_block_number();

        // [(relative_millis, input_median_time, input_committed_time, tip_median_time, expected_result)]
        let cases = vec![
            (1000, t + 2000, t + 3000, t + 5000, PASS),
            (1000, t + 2000, t + 3000, t + 4000, PASS),
            (1000, t + 2000, t + 4000, t + 4000, ERROR_IMMATURE),
            (1000, t + 2000, t + 3000, t + 1999, ERROR_IMMATURE),
        ];

        for (
            i,
            (
                relative_millis,
                input_median_time,
                input_committed_time,
                tip_median_time,
                expected_result,
            ),
        ) in cases.into_iter().enumerate()
        {
            assert!(input_median_time < input_committed_time);

            // Use a standalone node to run a case
            let node = node2021.clone_node(&format!("{}-cloned-{}", node2021.node_name(), i));
            ckb_testkit::info!(
                "[Node {}] run case-{}, old_tip_number: {}",
                node.node_name(),
                i,
                old_tip_number
            );

            let input_committed_number = old_tip_number + median_time_block_count + 1;
            let new_tip_number = input_committed_number + median_time_block_count;
            let mut instructions = vec![
                BuildInstruction::HeaderTimestamp {
                    template_number: input_committed_number
                        - (median_time_block_count - median_time_block_count / 2),
                    timestamp: input_median_time,
                },
                BuildInstruction::HeaderTimestamp {
                    template_number: input_committed_number,
                    timestamp: input_committed_time,
                },
                BuildInstruction::HeaderTimestamp {
                    template_number: new_tip_number
                        - (median_time_block_count - median_time_block_count / 2),
                    timestamp: tip_median_time,
                },
            ];
            // make sure the chain's timestamps are increasing
            for block_number in old_tip_number + 1..=new_tip_number {
                if !instructions
                    .iter()
                    .any(|ins| ins.template_number() == block_number)
                {
                    let mut timestamp = t + block_number - old_tip_number;
                    if let Some(parent_instruction) = instructions
                        .iter()
                        .find(|ins| ins.template_number() == block_number - 1)
                    {
                        if let BuildInstruction::HeaderTimestamp {
                            timestamp: parent_timestamp,
                            ..
                        } = parent_instruction
                        {
                            timestamp = parent_timestamp + 1;
                        }
                    }

                    instructions.push(BuildInstruction::HeaderTimestamp {
                        template_number: block_number,
                        timestamp,
                    });
                }
            }
            assert_eq!(
                instructions
                    .iter()
                    .map(|ins| ins.template_number())
                    .collect::<HashSet<_>>()
                    .len(),
                instructions.len()
            );
            node.build_according_to_instructions(new_tip_number, instructions)
                .unwrap_or_else(|err| {
                    panic!(
                        "failed to build case-{}, error: {}, current_tip_number: {}",
                        i,
                        err,
                        node.get_tip_block_number()
                    )
                });

            let since = since_from_relative_timestamp(relative_millis / 1000);
            let input = {
                let input_committed_block = node.get_block_by_number(input_committed_number);
                assert_eq!(input_committed_time, input_committed_block.timestamp());
                let cellbase = input_committed_block.transaction(0).unwrap();
                OutPoint::new(cellbase.hash(), 0)
            };
            let output = CellOutput::new_builder()
                .lock(node.always_success_script())
                .build_exact_capacity(Capacity::zero())
                .unwrap();
            let tx = TransactionBuilder::default()
                .input(CellInput::new(input, since))
                .output(output)
                .output_data(Default::default())
                .cell_dep(node.always_success_cell_dep())
                .build();
            let result = node
                .rpc_client()
                .send_transaction_result(tx.pack().data().into());
            if expected_result == PASS {
                assert!(
                    result.is_ok(),
                    "[Node {}] run case-{}, expect Ok but got {}",
                    node.node_name(),
                    i,
                    result.unwrap_err(),
                );
            } else {
                assert!(
                    result.is_err(),
                    "[Node {}] run case-{}, expect Err(\"{}\") but got Ok",
                    node.node_name(),
                    i,
                    expected_result
                );
                let err = result.unwrap_err();
                assert!(
                    err.to_string().contains(expected_result),
                    "[Node {}] run case-{}, expect Err(\"{}\") but got Err(\"{}\")",
                    node.node_name(),
                    i,
                    expected_result,
                    err,
                );
            }
        }
    }
}
