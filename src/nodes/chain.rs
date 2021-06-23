use crate::debug;
use crate::nodes::Nodes;
use crate::util::wait_until;
use std::collections::HashSet;

impl Nodes {
    pub fn waiting_for_sync(&self) {
        debug!("Nodes::waiting_for_sync start");
        let mut tip_blocks = HashSet::new();

        // 60 seconds is a reasonable timeout to sync, even for poor CI server
        let synced = wait_until(60, || {
            tip_blocks = self.nodes().map(|node| node.get_tip_block()).collect();
            tip_blocks.len() == 1
        });
        if !synced {
            let tips = self
                .nodes()
                .map(|node| {
                    let block = node.get_tip_block();
                    (node.node_name(), block.number(), block.hash())
                })
                .collect::<Vec<_>>();
            panic!(
                "timeout to wait for nodes sync to a same chain, tips: {:?}",
                tips,
            );
        }
        for node in self.nodes() {
            node.wait_for_tx_pool();
        }
        debug!("Nodes::waiting_for_sync end");
    }
}
