use ckb_testkit::Node;
use ckb_types::{packed, prelude::*};

pub(super) fn test_extension_via_size(
    node: &Node,
    extension_size: Option<usize>,
    expected: Result<(), &'static str>,
) {
    let template = node.rpc_client().get_block_template(None, None, None);
    let block = packed::Block::from(template)
        .as_advanced_builder()
        .extension(extension_size.map(|s| vec![0u8; s].pack()))
        .build();
    let actual = node
        .rpc_client()
        .submit_block("".to_owned(), block.data().into());
    match (expected, actual) {
        (Ok(()), Ok(_)) => {}
        (Err(errmsg), Err(err)) => {
            assert!(
                err.to_string().contains(errmsg),
                "expect Err(\".*{}.*\"), but got Err({:?})",
                errmsg,
                err
            );
        }
        (Ok(()), Err(err)) => {
            panic!("expect Ok(()), but got: Err({:?})", err)
        }
        (Err(errmsg), Ok(block_hash)) => {
            panic!(
                "expect Err(\".*{}.*\"), but got: Ok({:#x})",
                errmsg, block_hash
            )
        }
    }
}
