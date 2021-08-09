use crate::case::rfc0031::util::test_extension_via_size;
use crate::case::rfc0031::ERROR_UNKNOWN_FIELDS;
use crate::case::{Case, CaseOptions};
use crate::CKB2021;
use ckb_testkit::Nodes;
use ckb_testkit::{Node, NodeOptions};
use ckb_types::core::EpochNumber;

const RFC0224_EPOCH_NUMBER: EpochNumber = 3;

pub struct RFC0031V2019;

impl Case for RFC0031V2019 {
    fn case_options(&self) -> CaseOptions {
        CaseOptions {
            make_all_nodes_connected: true,
            make_all_nodes_synced: true,
            make_all_nodes_connected_and_synced: true,
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
        assert!(!is_rfc0224_switched(node2021));

        let cases = vec![
            (node2021, None, Ok(())),
            (node2021, Some(0), Err(ERROR_UNKNOWN_FIELDS)),
            (node2021, Some(1), Err(ERROR_UNKNOWN_FIELDS)),
            (node2021, Some(16), Err(ERROR_UNKNOWN_FIELDS)),
            (node2021, Some(32), Err(ERROR_UNKNOWN_FIELDS)),
            (node2021, Some(64), Err(ERROR_UNKNOWN_FIELDS)),
            (node2021, Some(96), Err(ERROR_UNKNOWN_FIELDS)),
            (node2021, Some(97), Err(ERROR_UNKNOWN_FIELDS)),
        ];
        for (node, extension_size, expected) in cases {
            assert!(!is_rfc0224_switched(node2021));
            test_extension_via_size(node, extension_size, expected);
            nodes
                .waiting_for_sync()
                .expect("nodes should be synced when they obey the same old rule");
        }
    }
}

fn is_rfc0224_switched(node: &Node) -> bool {
    node.rpc_client().get_current_epoch().number.value() >= RFC0224_EPOCH_NUMBER
}
