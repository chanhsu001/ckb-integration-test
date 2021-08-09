// ## Cases And Expect Results
//
// Before rfc0240, transactions are not allowed to reference a `HeaderDep` that born in last 4 epochs.

use crate::case::{Case, CaseOptions};
use crate::{CKB2019, CKB2021};
use ckb_jsonrpc_types::EpochNumberWithFraction;
use ckb_testkit::Nodes;
use ckb_testkit::{Node, NodeOptions};
use ckb_types::core::EpochNumber;
use ckb_types::prelude::Pack;

const RFC0240_EPOCH_NUMBER: EpochNumber = 3;

pub struct RFC0036V2019;

impl Case for RFC0036V2019 {
    fn case_options(&self) -> CaseOptions {
        CaseOptions {
            make_all_nodes_connected: true,
            make_all_nodes_synced: true,
            make_all_nodes_connected_and_synced: true,
            node_options: vec![
                NodeOptions {
                    node_name: String::from("node2019"),
                    ckb_binary: CKB2019.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V1TestData",
                    chain_spec: "testdata/spec/cellbase_maturity_not_zero_2019",
                    app_config: "testdata/config/ckb2019",
                },
                NodeOptions {
                    node_name: String::from("node2021"),
                    ckb_binary: CKB2021.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V2TestData",
                    chain_spec: "testdata/spec/cellbase_maturity_not_zero_2021",
                    app_config: "testdata/config/ckb2021",
                },
            ]
            .into_iter()
            .collect(),
        }
    }

    fn run(&self, nodes: Nodes) {
        let node2019 = nodes.get_node("node2019");
        let node2021 = nodes.get_node("node2021");
        assert!(!is_rfc0240_switched(node2019));
        assert!(node2019.consensus().cellbase_maturity > EpochNumberWithFraction::from(0));
        assert!(node2021.consensus().cellbase_maturity > EpochNumberWithFraction::from(0));

        let tip_hash = node2019.get_tip_block().hash();
        let tx = {
            let input = node2019.get_spendable_always_success_cells()[0].to_owned();
            let tx = node2019.always_success_transaction(&input);
            tx.as_advanced_builder().header_dep(tip_hash).build()
        };
        for node in nodes.nodes() {
            let result = node
                .rpc_client()
                .send_transaction_result(tx.pack().data().into());
            assert!(
                result.is_err()
                    && result
                        .as_ref()
                        .unwrap_err()
                        .to_string()
                        .contains("ImmatureHeader"),
                "before rfc0240, {} should reject tx with error \"ImmatureHeader\", but got: {:?}",
                node.node_name(),
                result,
            );
        }
    }
}

fn is_rfc0240_switched(node: &Node) -> bool {
    node.rpc_client().get_current_epoch().number.value() >= RFC0240_EPOCH_NUMBER
}
