use ckb_testkit::Node;
use ckb_types::core::cell::CellMeta;
use ckb_types::{
    core::{TransactionBuilder, TransactionView},
    packed::{CellInput, CellOutput},
    prelude::*,
};

pub(super) fn generate_transaction(node: &Node, input: &CellMeta) -> TransactionView {
    TransactionBuilder::default()
        .input(CellInput::new(input.out_point.clone(), 0))
        .output(
            CellOutput::new_builder()
                .lock(input.cell_output.lock())
                .type_(input.cell_output.type_())
                .capacity(input.capacity().pack())
                .build(),
        )
        .output_data(Default::default())
        .cell_dep(node.always_success_cell_dep())
        .build()
}
