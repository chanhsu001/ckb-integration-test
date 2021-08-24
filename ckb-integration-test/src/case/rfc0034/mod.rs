use ckb_types::core::EpochNumber;

pub(super) mod rfc0034;

pub const RFC0034_EPOCH_NUMBER: EpochNumber = 3;
const ERROR_INVALID_ECALL: &str = "InvalidEcall";
#[allow(dead_code)]
const ERROR_INVALID_VM_VERSION: &str = " Invalid VM Version";
#[allow(dead_code)]
const ERROR_OUT_OF_BOUND: &str = "error code 1 in the page";

const SOURCE_INPUT: u32 = 0x0000000000000001;
const SOURCE_OUTPUT: u32 = 0x0000000000000002;
const SOURCE_DEP: u32 = 0x0000000000000003;
const PLACE_CELL_DATA: u32 = 0;
const PLACE_WITNESS: u32 = 1;

// > * `output.type_.hash_type = "type"`, which means it always runs on the latest VM
// > * `output.type_.code_hash = exec_caller`
// > * `exec`'s parameter `bounds` is always be `0`
// > * `exec`'s parameter `index` is always be `0`
//
// ┌──────────┬───────────┬─────────────────────────────┬──────────────┬───────────────┐
// │          │           │                             │              │               │
// │  Source  │   Place   │ Transaction                 │  v2019       │  v2021        │
// │          │           │                             │              │               │
// ├──────────┼───────────┼─────────────────────────────┼──────────────┼───────────────┤
// │          │           │                             │              │               │
// │          │           │ input.data = null           │              │               │
// │  Output  │    Data   │ output.data = exec_callee   │ InvalidEcall │      Pass     │
// │          │           │ witness = null              │              │               │
// ├──────────┼───────────┼─────────────────────────────┼──────────────┼───────────────┤
// │          │           │                             │              │               │
// │          │           │ input.data = null           │              │      Pass     │
// │  Output  │  Witness  │ output.data = null          │ InvalidEcall │               │
// │          │           │ witness = exec_callee       │              │               │
// ├──────────┼───────────┼─────────────────────────────┼──────────────┼───────────────┤
// │          │           │                             │              │               │
// │          │           │ input.data = exec_callee    │              │      Pass     │
// │  Input   │    Data   │ output.data = null          │ InvalidEcall │               │
// │          │           │ witness = null              │              │               │
// ├──────────┼───────────┼─────────────────────────────┼──────────────┼───────────────┤
// │          │           │                             │              │               │
// │          │           │ input.data = null           │ InvalidEcall │      Pass     │
// │  Input   │  Witness  │ output.data = null          │              │               │
// │          │           │ witness = exec_callee       │              │               │
// ├──────────┼───────────┼─────────────────────────────┼──────────────┼───────────────┤
// │          │           │                             │              │               │
// │          │           │ input.data = null           │              │               │
// │ DepCell  │    Data   │ output.data = null          │ InvalidEcall │      Pass     │
// │          │           │ witness = null              │              │               │
// │          │           │ dep_cell.data = exec_callee │              │               │
// ├──────────┼───────────┼─────────────────────────────┼──────────────┼───────────────┤
// │          │           │                             │              │               │
// │          │           │ input.data = null           │              │               │
// │ DepCell  │  Witness  │ output.data = null          │ InvalidEcall │  OutOfBound   │
// │          │           │ witness = exec_callee       │              │      &        │
// │          │           │                             │              │  InvalidEcall │
// │          │           │                             │              │               │
// └──────────┴───────────┴─────────────────────────────┴──────────────┴───────────────┘
