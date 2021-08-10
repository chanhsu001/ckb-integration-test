use ckb_testkit::{Node, User, SIGHASH_ALL_DATA_HASH, SIGHASH_ALL_TYPE_HASH};
use ckb_types::core::{ScriptHashType, TransactionBuilder, TransactionView};
use ckb_types::packed::{CellDep, CellOutput, Script};
use ckb_types::{
    core::cell::CellMeta,
    packed::{Byte32, CellInput, OutPoint},
    prelude::*,
};
use crossbeam_channel::{Receiver, Sender};
use lru::LruCache;
use std::collections::HashMap;
use std::time::Instant;

pub struct LiveCellProducer {
    users: Vec<User>,
    nodes: Vec<Node>,
    seen_out_points: LruCache<OutPoint, Instant>,
}

// TODO Add more logs
impl LiveCellProducer {
    pub fn new(users: Vec<User>, nodes: Vec<Node>) -> Self {
        let n_users = users.len();
        Self {
            users,
            nodes,
            seen_out_points: LruCache::new(n_users + 10),
        }
    }

    pub fn run(mut self, live_cell_sender: Sender<CellMeta>) {
        loop {
            let min_tip_number = self
                .nodes
                .iter()
                .map(|node| node.get_tip_block_number())
                .min()
                .unwrap();
            for user in self.users.iter() {
                let live_cells = user
                    .get_spendable_single_secp256k1_cells(&self.nodes[0])
                    .into_iter()
                    // TODO reduce competition
                    .filter(|cell| {
                        if self.seen_out_points.contains(&cell.out_point) {
                            return false;
                        }
                        let tx_info = cell
                            .transaction_info
                            .as_ref()
                            .expect("live cell's transaction info should be ok");
                        if tx_info.block_number > min_tip_number {
                            return false;
                        }
                        true
                    })
                    .collect::<Vec<_>>();
                for cell in live_cells {
                    self.seen_out_points
                        .put(cell.out_point.clone(), Instant::now());
                    let _ignore = live_cell_sender.send(cell);
                }
            }
        }
    }
}

pub struct TransactionProducer {
    // #{ lock_hash => user }
    users: HashMap<Byte32, User>,
    cell_deps: Vec<CellDep>,
    n_inout: usize,
    // #{ lock_hash => live_cell }
    live_cells: HashMap<Byte32, CellMeta>,
    // #{ out_point => live_cell }
    backlogs: HashMap<OutPoint, CellMeta>,
}

impl TransactionProducer {
    pub fn new(users: Vec<User>, cell_deps: Vec<CellDep>, n_inout: usize) -> Self {
        let mut users_map = HashMap::new();
        for user in users {
            users_map.insert(
                user.single_secp256k1_lock_script_via_type()
                    .calc_script_hash(),
                user.clone(),
            );
            users_map.insert(
                user.single_secp256k1_lock_script_via_data()
                    .calc_script_hash(),
                user.clone(),
            );
            users_map.insert(
                user.single_secp256k1_lock_script_via_data1()
                    .calc_script_hash(),
                user.clone(),
            );
        }

        Self {
            users: users_map,
            cell_deps,
            n_inout,
            live_cells: HashMap::new(),
            backlogs: HashMap::new(),
        }
    }

    pub fn run(
        mut self,
        live_cell_receiver: Receiver<CellMeta>,
        transaction_sender: Sender<TransactionView>,
    ) {
        while let Ok(live_cell) = live_cell_receiver.recv() {
            let lock_hash = live_cell.cell_output.calc_lock_hash();
            match self.live_cells.entry(lock_hash.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => {
                    if entry.get().out_point == live_cell.out_point {
                        self.backlogs.insert(live_cell.out_point.clone(), live_cell);
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(live_cell);
                }
            }

            if self.live_cells.len() >= self.n_inout {
                let mut live_cells = HashMap::new();
                std::mem::swap(&mut self.live_cells, &mut live_cells);

                let inputs = live_cells
                    .values()
                    .map(|cell| {
                        CellInput::new_builder()
                            .previous_output(cell.out_point.clone())
                            .build()
                    })
                    .collect::<Vec<_>>();
                let outputs = live_cells
                    .values()
                    .map(|cell| {
                        // use tx_index as random number
                        let tx_index = cell.transaction_info.as_ref().unwrap().index;
                        let user = self.users.get(&lock_hash).expect("should be ok");
                        match tx_index % 3 {
                            0 => CellOutput::new_builder()
                                .capacity((cell.capacity().as_u64() - 1000).pack())
                                .lock(user.single_secp256k1_lock_script_via_data())
                                .build(),
                            1 => CellOutput::new_builder()
                                .capacity((cell.capacity().as_u64() - 1000).pack())
                                .lock(user.single_secp256k1_lock_script_via_type())
                                .build(),
                            2 => CellOutput::new_builder()
                                .capacity((cell.capacity().as_u64() - 1000).pack())
                                .lock(user.single_secp256k1_lock_script_via_data1())
                                .build(),
                            _ => unreachable!(),
                        }
                    })
                    .collect::<Vec<_>>();
                let outputs_data = live_cells.values().map(|_| Default::default());
                let raw_tx = TransactionBuilder::default()
                    .inputs(inputs)
                    .outputs(outputs)
                    .outputs_data(outputs_data)
                    .cell_deps(self.cell_deps.clone())
                    .build();
                let witnesses = live_cells.values().map(|cell| {
                    let lock_hash = cell.cell_output.calc_lock_hash();
                    let user = self.users.get(&lock_hash).expect("should be ok");
                    user.single_secp256k1_signed_witness(&raw_tx)
                        .as_bytes()
                        .pack()
                });
                let signed_tx = raw_tx.as_advanced_builder().witnesses(witnesses).build();

                if transaction_sender.send(signed_tx).is_err() {
                    // SendError occurs, the corresponding transaction receiver is dead
                    return;
                }

                let mut backlogs = HashMap::new();
                std::mem::swap(&mut self.backlogs, &mut backlogs);
                for (_, live_cell) in backlogs.into_iter() {
                    let lock_hash = live_cell.cell_output.calc_lock_hash();
                    match self.live_cells.entry(lock_hash) {
                        std::collections::hash_map::Entry::Occupied(entry) => {
                            if entry.get().out_point == live_cell.out_point {
                                self.backlogs.insert(live_cell.out_point.clone(), live_cell);
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(live_cell);
                        }
                    }
                }
            }
        }
    }
}
