// ## Cases And Expect Results
//
// After fork2021, nodes2019s disconnect node2021s, node2021s disconnect node2019s.
//
// Check the connections via RPC `get_peers`

use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::{CKB2019, CKB2021};
use ckb_testkit::util::wait_until;
use ckb_testkit::Nodes;
use ckb_testkit::{Node, NodeOptions};
use ckb_types::core::EpochNumber;

const RFC0234_EPOCH_NUMBER: EpochNumber = 3;

pub struct RFC0035V2021Connection;

impl Case for RFC0035V2021Connection {
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
                    chain_spec: "testdata/spec/ckb2019",
                    app_config: "testdata/config/ckb2019",
                },
                NodeOptions {
                    node_name: String::from("node2019_2"),
                    ckb_binary: CKB2019.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V1TestData",
                    chain_spec: "testdata/spec/ckb2019",
                    app_config: "testdata/config/ckb2019",
                },
                NodeOptions {
                    node_name: String::from("node2021"),
                    ckb_binary: CKB2021.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V2TestData",
                    chain_spec: "testdata/spec/ckb2021",
                    app_config: "testdata/config/ckb2021",
                },
                NodeOptions {
                    node_name: String::from("node2021_2"),
                    ckb_binary: CKB2021.read().unwrap().clone(),
                    initial_database: "testdata/db/Epoch2V2TestData",
                    chain_spec: "testdata/spec/ckb2021",
                    app_config: "testdata/config/ckb2021",
                },
                // NodeOptions {
                //     node_name: String::from("node2021_non_hardfork"),
                //     ckb_binary: CKB2021.read().unwrap().clone(),
                //     initial_database: "testdata/db/Epoch2V2TestData",
                //     chain_spec: "testdata/spec/non_hardfork_2021",
                //     app_config: "testdata/config/ckb2021",
                // },
            ]
            .into_iter()
            .collect(),
        }
    }

    fn run(&self, nodes: Nodes) {
        let node2021 = nodes.get_node("node2021");
        node2021.mine_to(calc_epoch_start_number(node2021, RFC0234_EPOCH_NUMBER) - 1);
        nodes
            .waiting_for_sync()
            .expect("nodes should be synced before rfc0234.switch");

        node2021.mine_to(calc_epoch_start_number(node2021, RFC0234_EPOCH_NUMBER));
        let disconnect_different_version_nodes = wait_until(20, || {
            nodes.nodes().all(|node| {
                let local_node_info = node.rpc_client().local_node_info();
                node.rpc_client()
                    .get_peers()
                    .iter()
                    .all(|peer| local_node_info.version == peer.version)
            })
        });
        if !disconnect_different_version_nodes {
            for node in nodes.nodes() {
                let local_node_info = node.rpc_client().local_node_info();
                for peer in node.rpc_client().get_peers() {
                    if local_node_info.version != peer.version {
                        panic!(
                            "nodes with different fork versions should be disconnected, but {}({}) still connect with {}({})",
                            node.node_name(), local_node_info.version, peer.node_id, peer.version,
                        );
                    }
                }
            }
        }

        // TODO Actually, the below check is for SyncProtocol
        let mut fresh_node2021 = {
            let node_options = NodeOptions {
                node_name: String::from("fresh_node2021"),
                ckb_binary: CKB2021.read().unwrap().clone(),
                initial_database: "testdata/db/empty",
                chain_spec: "testdata/spec/ckb2021",
                app_config: "testdata/config/ckb2021",
            };
            Node::init(self.case_name(), node_options, true)
        };
        fresh_node2021.start();
        fresh_node2021.p2p_connect(node2021);
        let synced = wait_until(180, || {
            fresh_node2021.get_tip_block_number() == node2021.get_tip_block_number()
        });
        assert!(
            synced,
            "fresh_node2021 should sync from node2021s, fresh_node2021.tip: {}, node2021.tip: {}",
            fresh_node2021.get_tip_block_number(),
            node2021.get_tip_block_number(),
        );

        // TODO Actually, the below check is for RelayProtocol
        // let node2021_non_hardfork = nodes.get_node("node2021_non_hardfork");
        // let tx = {
        //     let input = node2021.get_spendable_always_success_cells()[0].to_owned();
        //     node2021.always_success_transaction(&input)
        // };
        // node2021.submit_transaction(&tx);
        // let tx_relayed = wait_until(30, || node2021_non_hardfork.is_transaction_pending(&tx));
        // assert!(!tx_relayed, "tx should be unable to relay between node2021s");
    }
}
