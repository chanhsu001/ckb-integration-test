// ## Cases And Expect Results
//
// RelayTransaction is used to propagate transactions over network.
// After fork2021, nodes2019s disconnect node2021s, node2021s disconnect node2019s, so transactions
// cannot be propagated among nodes with different fork versions.
// After fork2021, transactions can be propagated among nodes with the same fork versions.

use crate::case::rfc0035::util::generate_transaction;
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::{CKB2019, CKB2021};
use ckb_testkit::util::wait_until;
use ckb_testkit::{NodeOptions, Nodes};
use ckb_types::core::EpochNumber;

const RFC0234_EPOCH_NUMBER: EpochNumber = 3;

pub struct RFC0035V2021RelayTransaction;

impl Case for RFC0035V2021RelayTransaction {
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
            ]
            .into_iter()
            .collect(),
        }
    }

    fn run(&self, nodes: Nodes) {
        let node2019 = nodes.get_node("node2019");
        let node2021 = nodes.get_node("node2021");
        let node2019s = nodes
            .nodes()
            .filter(|node| node.node_options().ckb_binary == *CKB2019.read().unwrap())
            .collect::<Vec<_>>();
        let node2021s = nodes
            .nodes()
            .filter(|node| node.node_options().ckb_binary == *CKB2021.read().unwrap())
            .collect::<Vec<_>>();
        let cells = node2019.get_spendable_always_success_cells();
        let tx1 = generate_transaction(node2019, &cells[0]);
        let tx2 = generate_transaction(node2019, &cells[1]);

        node2021.mine_to(calc_epoch_start_number(node2021, RFC0234_EPOCH_NUMBER) - 1);
        nodes
            .waiting_for_sync()
            .expect("nodes should be synced before rfc0234.switch");

        node2021.mine_to(calc_epoch_start_number(node2021, RFC0234_EPOCH_NUMBER));
        {
            node2019s[0].submit_transaction(&tx1);
            let propagated_among_node2019s = wait_until(20, || {
                node2019s
                    .iter()
                    .all(|node| node.is_transaction_pending(&tx1))
            });
            assert!(
                propagated_among_node2019s,
                "node2019s should receive tx1 from \"node2019\"",
            );
            node2019s[0].mine(node2019s[0].consensus().tx_proposal_window.closest.value() + 1);
            let packaged_among_node2019s = wait_until(20, || {
                node2019s
                    .iter()
                    .all(|node| node.is_transaction_committed(&tx1))
            });
            assert!(packaged_among_node2019s, "node2019s should commit tx1",);
        }

        {
            node2021s[0].submit_transaction(&tx2);
            let propagated_among_node2021s = wait_until(20, || {
                node2021s
                    .iter()
                    .all(|node| node.is_transaction_pending(&tx2))
            });
            assert!(
                propagated_among_node2021s,
                "node2021s should receive tx2 from \"node2021\"",
            );
            node2021s[0].mine(node2021s[0].consensus().tx_proposal_window.closest.value() + 1);
            let packaged_among_node2021s = wait_until(20, || {
                node2021s
                    .iter()
                    .all(|node| node.is_transaction_committed(&tx2))
            });
            assert!(packaged_among_node2021s, "node2021s should commit tx2",);
        }
    }
}
