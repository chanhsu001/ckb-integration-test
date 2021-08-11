use ckb_testkit::Node;
use ckb_types::core::EpochNumber;

pub(super) mod util;
pub(super) mod v2019;
pub(super) mod v2021;

const RFC0029_EPOCH_NUMBER: EpochNumber = 3;
const PASS: &str = "Pass";
const ERROR_MULTIPLE_MATCHES: &str = "MultipleMatches";
const ERROR_DUPLICATE_CELL_DEPS: &str = "DuplicateCellDeps";

fn is_rfc0029_switched(node: &Node) -> bool {
    node.rpc_client().get_current_epoch().number.value() >= RFC0029_EPOCH_NUMBER
}

// ## [RFC0029](https://github.com/nervosnetwork/rfcs/pull/222)
//
// ### Cases
//
// * `a1`, `a2` and `b1` are 3 cells
// * `a1` and `a2` have the same output-data
// * `a1`, `a2` and `b1` have the same type-script
// * `Group(x, y, ..)` indicates a `DepGroup` points to `x` and `y` cells
// * when `script.hash_type` is `"data"`, `script.code_hash` is always `a1.data_hash`;
//   when `script.hash_type` is `"type"`, `script.code_hash` is always `a1.type_hash`
//
// | script.hash_type | cell_deps  | 2019   | 2021   |
// | :---- | :----  | ----:  | ---: |
// | "data" | `[a1]` | Pass | Pass |
// | "data" | `[a1, a1]` | DuplicateCellDeps | DuplicateCellDeps |
// | "data" | `[a1, a2]` | Pass | Pass |
// | "data" | `[a1, b1]` | Pass | Pass |
// | "data" | `[Group(a1)]` | Pass | Pass |
// | "data" | `[Group(a1, a1)]` | Pass | Pass |
// | "data" | `[Group(a1, a2)]` | Pass | Pass |
// | "data" | `[Group(a1, b1)]` | Pass | Pass |
// | "data" | `[Group(a1), a1]` | Pass | Pass |
// | "data" | `[Group(a1), a2]` | Pass | Pass |
// | "data" | `[Group(a1), b1]` | Pass | Pass |
// | "data" | `[Group(a1), Group(a2)]` | Pass | Pass |
// | "data" | `[Group(a1), Group(b1)]` | Pass | Pass |
// | "type" | `[a1]` | Pass | Pass |
// | "type" | `[a1, a1]` | DuplicateCellDeps | DuplicateCellDeps |
// | "type" | `[a1, a2]` | MultipleMatches | Pass |
// | "type" | `[a1, b1]` | MultipleMatches | MultipleMatches |
// | "type" | `[Group(a1)]` | Pass | Pass |
// | "type" | `[Group(a1, a1)]` | MultipleMatches | Pass |
// | "type" | `[Group(a1, a2)]` | MultipleMatches | Pass |
// | "type" | `[Group(a1, b1)]` | MultipleMatches | MultipleMatches |
// | "type" | `[Group(a1), a1]` | MultipleMatches | Pass |
// | "type" | `[Group(a1), a2]` | MultipleMatches | Pass |
// | "type" | `[Group(a1), b1]` | MultipleMatches | MultipleMatches |
// | "type" | `[Group(a1), Group(a2)]` | MultipleMatches | Pass |
// | "type" | `[Group(a1), Group(b1)]` | MultipleMatches | MultipleMatches |
