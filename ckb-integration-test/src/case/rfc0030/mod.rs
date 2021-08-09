mod util;
pub(super) mod v2019;
pub(super) mod v2021;

const ERROR_INVALID_SINCE: &str = "InvalidSince";
const ERROR_IMMATURE: &str = "Immature";

// ## [RFC0030](https://github.com/nervosnetwork/rfcs/pull/223)
//
// ### Cases
//
// > * `t` indicates the block epoch that committed the input out-point, `t = input.tx_info.block`
// > * `abs(x, y, z)` is shortcut of `since` that
// >   - `metric_flag` is epoch (01)
// >   - `relative_flag` is absolute (0)
// >   - `value` is `EpochNumberWithFraction { number = x, index = y, length = z }`
// > * `rel(x, y, z)` is shortcut of `since` that
// >   - `metric_flag` is epoch (01)
// >   - `relative_flag` is relative (1)
// >   - `value` is `EpochNumberWithFraction { number = x, index = y, length = z }`
// > epoch length is always `1000`
//
// | id |since.epoch (number, index, length)                      | v2019 | v2021 |
// | :--- |:---                                                     | :--- | :--- |
// | 0 |abs(2, 0, 0)                                             | Pass(2, 0, 1000)    | <- |
// | 1 |abs(2, 1, 0)                                             | Pass(2, 0, 1000)    | InvalidSince      |
// | 2 |abs(2, 0, 1)                                             | Pass(2, 0, 1000)    |    <-    |
// | 3 |abs(1, 1, 1)                                             | Pass(2, 0, 1000)    |  InvalidSince     |
// | 4 |abs(0, 2, 1)                                             | Pass(1, 0, 1000)?   |  InvalidSince     |
// | 5 |abs(2, 1, 2)                                             | Pass(2, 500, 1000)  |    <-    |
// | 6 |rel(0, 0, 0)                                             | Pass(t.epoch.number, t.epoch.index, 1000)      |  <-|
// | 7 |rel(0, 1, 0)                                             | Pass(t.epoch.number, t.epoch.index, 1000)      | InvalidSince      |
// | 8 |rel(0, 0, 1)                                             | Pass(t.epoch.number, t.epoch.index, 1000)      |    <-    |
// | 9 |rel(0, 1, 1)                                             | Pass(t.epoch.number + 1, t.epoch.index, 1000)      | InvalidSince      |
// | 10 |rel(0, 2, 1)                                             | Pass(t.epoch.number + 2, t.epoch.index, 1000)      |    InvalidSince   |
// | 11 |rel(0, 1, 2)                                             | Pass(t.epoch.number, t.epoch.index + 500, 1000)      |     <-   |
