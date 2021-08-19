use super::RFC0028_EPOCH_NUMBER;
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::CKB2021;
use ckb_testkit::util::since_from_relative_block_number;
use ckb_testkit::NodeOptions;
use ckb_testkit::{BuildInstruction, Nodes};
use ckb_types::core::Capacity;
use ckb_types::packed::OutPoint;
use ckb_types::{
    core::TransactionBuilder,
    packed::{CellInput, CellOutput},
    prelude::*,
};

pub struct RFC0028Chained;

impl Case for RFC0028Chained {
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

        let inputs = node2021.get_spendable_always_success_cells();
        let tx1 = node2021.always_success_transaction(&inputs[0]);
        let since = since_from_relative_block_number(0);
        let tx2 = TransactionBuilder::default()
            .input(CellInput::new(OutPoint::new(tx1.hash(), 0), since))
            .output(
                CellOutput::new_builder()
                    .lock(node2021.always_success_script())
                    .build_exact_capacity(Capacity::zero())
                    .unwrap(),
            )
            .output_data(Default::default())
            .cell_dep(node2021.always_success_cell_dep())
            .build();

        let current_tip_number = node2021.get_tip_block_number();
        let instructions = vec![
            BuildInstruction::Propose {
                template_number: current_tip_number + 1,
                proposal_short_id: tx1.proposal_short_id(),
            },
            BuildInstruction::Propose {
                template_number: current_tip_number + 1,
                proposal_short_id: tx2.proposal_short_id(),
            },
            BuildInstruction::Commit {
                template_number: current_tip_number + 3,
                transaction: tx1.clone(),
            },
            BuildInstruction::Commit {
                template_number: current_tip_number + 3,
                transaction: tx2.clone(),
            },
        ];
        node2021
            .build_according_to_instructions(current_tip_number + 3, instructions)
            .expect("chained transaction with since_from_relative_block_number is ok");
    }
}
