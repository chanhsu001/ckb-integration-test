use crate::case::rfc0031::util::test_extension_via_size;
use crate::case::rfc0031::{ERROR_EMPTY_EXT, ERROR_MAX_LIMIT};
use crate::case::{Case, CaseOptions};
use crate::util::calc_epoch_start_number;
use crate::CKB2021;
use ckb_testkit::{NodeOptions, Nodes};
use ckb_types::core::EpochNumber;

const RFC0224_EPOCH_NUMBER: EpochNumber = 3;

pub struct RFC0031V2021;

impl Case for RFC0031V2021 {
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
        node2021.mine_to(calc_epoch_start_number(node2021, RFC0224_EPOCH_NUMBER));
        let cases = vec![
            (node2021, None, Ok(())),
            (node2021, Some(0), Err(ERROR_EMPTY_EXT)),
            (node2021, Some(1), Ok(())),
            (node2021, Some(16), Ok(())),
            (node2021, Some(32), Ok(())),
            (node2021, Some(64), Ok(())),
            (node2021, Some(96), Ok(())),
            (node2021, Some(97), Err(ERROR_MAX_LIMIT)),
        ];
        for (node, extension_size, expected) in cases {
            test_extension_via_size(node, extension_size, expected);
            nodes
                .waiting_for_sync()
                .expect("nodes should be synced when they obey the same old rule");
        }
    }
}
