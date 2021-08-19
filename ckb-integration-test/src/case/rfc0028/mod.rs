pub(super) mod chained;
pub(super) mod v2019;
pub(super) mod v2021;

// TODO Nodes in same case should have the same `initial_database`
// TODO Db version is related to ckb binary version. How to solve it?
const PASS: &str = "Pass";
const ERROR_IMMATURE: &str = "Immature";
const RFC0028_EPOCH_NUMBER: u64 = 3;

// ## [RFC0028](https://github.com/nervosnetwork/rfcs/pull/221)
//
// ### Cases
//
// Pre-condition:
// > * `tx_median_time <= tx_committed_time`
// > * The `tx.input.since.metric_flag` is block timestamp (10).
// > * The `tx.input.since.relative_flag` is relative (1).
//
// 0. `tx_median_time + relative_secs < tx_committed_time + relative_secs < tip_median_time`, v2019 Pass, v2021 Pass
// 1. `tx_median_time + relative_secs < tx_committed_time + relative_secs = tip_median_time`, v2019 Pass, v2021 Pass
// 2. `tx_median_time + relative_secs < tip_median_time < tx_committed_time + relative_secs`, v2019 Pass, v2021 Immature
// 3. `tip_median_time < tx_median_time + relative_secs < tx_committed_time + relative_secs`, v2019 Immature, v2021 Immature
//
// Here is table corresponding to the below cases:
//
// > * Set `T` as the origin timestamp
//
// | id | since relative | `tx_median_time` | `tx_committed_time` | `tip_median_time` | v2019    | v2021    |
// | :- | :-             | :-:              | :-:                 | :-:               | :-:      | :-:      |
// | 0 | 1s             | T+2s             |   T+3s              |  T+5s             |  Pass    | Pass     |
// | 1 | 1s             | T+2s             |   T+3s              |  T+4s             |  Pass    | Pass     |
// | 2 | 1s             | T+2s             |   T+4s              |  T+4s             |  Pass    | Immature |
// | 3 | 1s             | T+2s             |   T+3s              |  T+1999ms             |  Immature| Immature |
