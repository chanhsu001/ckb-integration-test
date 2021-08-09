// ## Cases And Expect Results
//
// After fork2021, node2021s will still propagate info

use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::CKB2021;
use ckb_testkit::util::wait_until;
use ckb_testkit::{NodeOptions, Nodes};
use ckb_types::core::EpochNumber;

const RFC0234_EPOCH_NUMBER: EpochNumber = 3;

pub struct RFC0035V2021Discovery;

impl Case for RFC0035V2021Discovery {
    fn case_options(&self) -> CaseOptions {
        CaseOptions {
            make_all_nodes_connected: false,
            make_all_nodes_synced: false,
            make_all_nodes_connected_and_synced: false,
            node_options: vec![
                NodeOptions {
                    node_name: String::from("node2021_1"),
                    ckb_binary: CKB2021.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V2TestData",
                    chain_spec: "testdata/spec/ckb2021",
                    app_config: "testdata/config/connect_outbound_interval_secs",
                },
                NodeOptions {
                    node_name: String::from("node2021_2"),
                    ckb_binary: CKB2021.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V2TestData",
                    chain_spec: "testdata/spec/ckb2021",
                    app_config: "testdata/config/connect_outbound_interval_secs",
                },
                NodeOptions {
                    node_name: String::from("node2021_3"),
                    ckb_binary: CKB2021.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V2TestData",
                    chain_spec: "testdata/spec/ckb2021",
                    app_config: "testdata/config/connect_outbound_interval_secs",
                },
            ]
            .into_iter()
            .collect(),
        }
    }

    fn run(&self, nodes: Nodes) {
        // Move node2021s beyond fork2021
        for node in nodes.nodes() {
            node.mine_to(calc_epoch_start_number(node, RFC0234_EPOCH_NUMBER));
        }
        for node in nodes.nodes() {
            assert!(node.rpc_client().get_peers().is_empty());
        }
        let node2021_1 = nodes.get_node("node2021_1");
        let node2021_2 = nodes.get_node("node2021_2");
        let node2021_3 = nodes.get_node("node2021_3");

        // NOTE: Currently, only inbound will query outbound via
        // `DiscoveryMessage::GetNodes`, but outbound will not do this at the vise direction.
        // So you cannot connect like below:
        // ```
        // node2021_1.p2p_connect(node2021_2);
        // node2021_1.p2p_connect(node2021_3);
        // ```
        node2021_2.p2p_connect(node2021_1);
        node2021_3.p2p_connect(node2021_1);

        let is_connected = wait_until(20, || {
            node2021_2
                .rpc_client()
                .get_peers()
                .iter()
                .any(|peer| &peer.node_id == node2021_3.node_id())
        });
        assert!(
            is_connected,
            "node2021_1 should propagate other 2021's info, so node2021_2 can connect to node2021_3"
        );
    }
}
