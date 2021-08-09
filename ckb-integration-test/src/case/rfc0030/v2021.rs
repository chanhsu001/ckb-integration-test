use crate::case::rfc0030::util::run_rfc0030_case;
use crate::case::rfc0030::ERROR_INVALID_SINCE;
use crate::case::{Case, CaseOptions};
use crate::CKB2021;
use ckb_testkit::util::{
    since_from_absolute_epoch_number_with_fraction, since_from_relative_epoch_number_with_fraction,
};
use ckb_testkit::NodeOptions;
use ckb_testkit::Nodes;
use ckb_types::core::Capacity;
use ckb_types::packed::OutPoint;
use ckb_types::{
    core::{EpochNumberWithFraction, TransactionBuilder},
    packed::{CellInput, CellOutput},
    prelude::*,
};

// Use spec "testdata/spec/ckb2021_params_hardfork_0"

pub struct RFC0030V2021;

impl Case for RFC0030V2021 {
    fn case_options(&self) -> CaseOptions {
        CaseOptions {
            make_all_nodes_connected: false,
            make_all_nodes_synced: false,
            make_all_nodes_connected_and_synced: false,
            node_options: vec![NodeOptions {
                node_name: String::from("node2021"),
                ckb_binary: CKB2021.read().unwrap().clone(),
                initial_database: "testdata/db/empty",
                chain_spec: "testdata/spec/ckb2021_params_hardfork_0",
                app_config: "testdata/config/ckb2021",
            }]
            .into_iter()
            .collect(),
        }
    }

    fn run(&self, nodes: Nodes) {
        let node2021 = nodes.get_node("node2021");
        node2021.mine(node2021.consensus().tx_proposal_window.farthest.value() + 4);

        let input_block = node2021.get_tip_block();

        let cases = vec![
            (
                since_from_absolute_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(2, 0, 0),
                ),
                Ok(EpochNumberWithFraction::new_unchecked(2, 0, 1000)),
            ),
            (
                since_from_absolute_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(2, 1, 0),
                ),
                Err(ERROR_INVALID_SINCE),
            ),
            (
                since_from_absolute_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(2, 0, 1),
                ),
                Ok(EpochNumberWithFraction::new_unchecked(2, 0, 1000)),
            ),
            (
                since_from_absolute_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(1, 1, 1),
                ),
                Err(ERROR_INVALID_SINCE),
            ),
            (
                since_from_absolute_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 2, 1),
                ),
                Err(ERROR_INVALID_SINCE),
            ),
            (
                since_from_absolute_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(2, 1, 2),
                ),
                Ok(EpochNumberWithFraction::new_unchecked(
                    2,
                    1000 * 1 / 2,
                    1000,
                )),
            ),
            (
                since_from_relative_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 0, 0),
                ),
                Ok(EpochNumberWithFraction::new_unchecked(
                    0,
                    input_block.epoch().index(),
                    1000,
                )),
            ),
            (
                since_from_relative_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 1, 0),
                ),
                Err(ERROR_INVALID_SINCE),
            ),
            (
                since_from_relative_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 0, 1),
                ),
                Ok(EpochNumberWithFraction::new_unchecked(
                    0,
                    input_block.epoch().index(),
                    1000,
                )),
            ),
            (
                since_from_relative_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 1, 1),
                ),
                Err(ERROR_INVALID_SINCE),
            ),
            (
                since_from_relative_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 2, 1),
                ),
                Err(ERROR_INVALID_SINCE),
            ),
            (
                since_from_relative_epoch_number_with_fraction(
                    EpochNumberWithFraction::new_unchecked(0, 1, 2),
                ),
                Ok(EpochNumberWithFraction::new_unchecked(
                    0,
                    1000 / 2 + input_block.epoch().index(),
                    1000,
                )),
            ),
        ];
        for (case, (since, expected)) in cases.into_iter().enumerate() {
            let input = {
                let cellbase = input_block.transaction(0).expect("cellbase");
                let out_point = OutPoint::new(cellbase.hash(), 0);
                CellInput::new(out_point, since)
            };
            let output = CellOutput::new_builder()
                .lock(node2021.always_success_script())
                .build_exact_capacity(Capacity::zero())
                .unwrap();
            let tx = TransactionBuilder::default()
                .input(input)
                .output(output)
                .output_data(Default::default())
                .cell_dep(node2021.always_success_cell_dep())
                .build();

            let cloned_node2021 = node2021.clone_node(&format!("node2021-case-{}", case));
            run_rfc0030_case(&cloned_node2021, case, &expected, &tx);
        }
    }
}
