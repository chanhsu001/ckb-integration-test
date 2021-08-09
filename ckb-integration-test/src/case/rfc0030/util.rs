use super::ERROR_IMMATURE;
use ckb_testkit::BuildInstruction;
use ckb_testkit::Node;
use ckb_types::core::EpochNumberWithFraction;
use ckb_types::core::TransactionView;

pub fn run_rfc0030_case(
    node: &Node,
    case: usize,
    expected: &Result<EpochNumberWithFraction, &str>,
    tx: &TransactionView,
) {
    loop {
        let actual = node.rpc_client().send_transaction_result(tx.data().into());

        if let Err(ref expected_error) = expected {
            assert!(
                actual.is_err(),
                "[Node {}] case-{} expected Err(\"{}\") but got Ok",
                node.node_name(),
                case,
                expected_error
            );
            assert!(
                actual
                    .as_ref()
                    .unwrap_err()
                    .to_string()
                    .contains(expected_error),
                "[Node {}] case-{} expected Err(\"{}\") but got {}",
                node.node_name(),
                case,
                expected_error,
                actual.as_ref().unwrap_err(),
            );
            return;
        }

        let expected_tip_epoch = expected.unwrap();

        if let Err(ref actual_error) = actual {
            assert!(
                actual_error.to_string().contains(ERROR_IMMATURE),
                "[Node {}] case-{} expected Ok({}) but got {}",
                node.node_name(),
                case,
                expected_tip_epoch,
                actual_error,
            );

            // immature error, continue next block
            node.mine(1);
            continue;
        }

        if actual.is_ok() {
            let actual_tip_epoch = node.get_tip_block().epoch();
            assert_eq!(
                expected_tip_epoch,
                actual_tip_epoch,
                "[Node {}] case-{} expected_tip_epoch: {}, actual_tip_epoch: {}",
                node.node_name(),
                case,
                expected_tip_epoch,
                actual_tip_epoch,
            );
        }

        break;
    }

    // test committing
    if expected.is_ok() && node.rpc_client().ckb2021 {
        let instructions = vec![
            BuildInstruction::Propose {
                block_number: node.get_tip_block_number() + 1,
                proposal_short_id: tx.proposal_short_id(),
            },
            BuildInstruction::Commit {
                block_number: node.get_tip_block_number() + 3,
                transaction: tx.clone(),
            },
        ];
        node.build_according_to_instructions(node.get_tip_block_number() + 3, instructions)
            .unwrap_or_else(|err| {
                panic!(
                    "[Node {}] case-{} failed to build_according_to_instructions, error: {}",
                    node.node_name(),
                    case,
                    err
                )
            });
    }
}
