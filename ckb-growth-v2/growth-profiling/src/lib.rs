use ckb_types::{
    bytes::Bytes,
    core::{Capacity, TransactionBuilder, TransactionView},
    packed::{CellDep, CellInput, CellOutput},
    prelude::*,
};

use growth_utils::{attach_witness, Account, MIN_CELL_CAP, MIN_FEE_RATE};

///generate accounts, wrapped owner account and derived accounts
pub fn generate_accounts(base: Account, acc_count: u16) -> Vec<Account> {
    let mut accounts = vec![base];
    for i in 1..acc_count {
        let new_account = accounts[(i - 1) as usize].derive_new_account();
        // let new_account = accounts[(i - 1) as usize].clone();
        accounts.push(new_account);
    }
    accounts
}

/// create specific number of live cells
///
/// build 1in-Nout transaction to create N output_cell out of 1 input_cell on one account
/// the 1st cell capacity is nearly equal to input cell, the other cells capacity is tiny
pub fn gen_live_cells(
    input: CellInput,
    accounts: &mut [Account],
    cell_cnt: u16,
    secp_cell_deps: &[CellDep],
) -> TransactionView {
    assert_eq!(cell_cnt as usize, accounts.len());
    let owner_account = &mut accounts[0];

    // we keep capacity in this account cause it's simple
    let origin_cap = Capacity::zero()
        .safe_add(owner_account.cell_cap)
        .expect("origin capacity");
    let rest = origin_cap
        .safe_sub(MIN_FEE_RATE as u64)
        .expect("for min_fee_rate");
    let cell_cap = Capacity::zero()
        .safe_add(2 * MIN_CELL_CAP)
        .expect("cell_cap");
    let sum_cell_cap = cell_cap.safe_mul(cell_cnt).expect("cell_cap multiple");
    let rest = rest
        .safe_sub(sum_cell_cap)
        .expect("sub live cells capacity");
    owner_account.cell_cap = rest.as_u64();

    let mut outputs = vec![CellOutput::new_builder()
        .capacity(owner_account.cell_cap.pack())
        .lock(owner_account.lock_args.clone())
        .build()];
    (0..cell_cnt).for_each(|i| {
        outputs.push(
            CellOutput::new_builder()
                .capacity((2 * MIN_CELL_CAP).pack())
                .lock(accounts[i as usize].lock_args.clone())
                .build(),
        );
    });

    let mut outputs_data = vec![];
    (0..cell_cnt + 1).for_each(|i| {
        outputs_data.push(Bytes::from(i.to_le_bytes().to_vec()));
    });

    let secp_cell_deps= Vec::from(secp_cell_deps);
    let tx = TransactionBuilder::default()
        .input(input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_deps(secp_cell_deps)
        .build();
    let accounts = [accounts[0].clone()];
    attach_witness(tx, &accounts)
}

/// create specific number of 2in2out txs
pub fn create_2in2out_txs(
    inputs: Vec<CellInput>,
    two_two_accounts: &mut [Account],
    txs_cnt: u16,
    cell_dep: &[CellDep],
) -> Vec<TransactionView> {
    let mut txs = vec![];

    (0..txs_cnt)
        .zip(two_two_accounts.chunks(2))
        .zip(inputs.chunks(2))
        .for_each(|((_, two_accounts), two_inputs)| {
            let new_tx = {
                let mut inputs = vec![];
                inputs.extend_from_slice(two_inputs);

                let mut outputs = vec![];
                for account in two_accounts.iter() {
                    outputs.push(
                        CellOutput::new_builder()
                            .capacity(MIN_CELL_CAP.pack())
                            .lock(account.lock_args.clone())
                            .build(),
                    );
                }

                let mut outputs_data = vec![];
                (0..2_u8).for_each(|i| {
                    outputs_data.push(Bytes::from(i.to_le_bytes().to_vec()));
                });

                let cell_dep = Vec::from(cell_dep);
                let tx = TransactionBuilder::default()
                    .inputs(inputs)
                    .outputs(outputs)
                    .outputs_data(outputs_data.pack())
                    .cell_deps(cell_dep)
                    .build();

                // let accounts = [two_accounts[0].clone()];
                attach_witness(tx, two_accounts)
            };

            txs.push(new_tx)
        });

    txs
}
