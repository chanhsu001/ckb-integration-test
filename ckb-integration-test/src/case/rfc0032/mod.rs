use ckb_types::core::EpochNumber;

pub(super) mod rfc0032;

// Occurs when specify output's scripts with `ScriptHashType = Data1` before rfc0032
const ERROR_INCOMPATIBLE: &str = "Compatible";
// Occurs when verify type script or unlock script, indicates that `ScriptHashType` is not supported.
const ERROR_INVALID_VM_VERSION: &str = "Invalid VM Version";
// RPCError, ScriptHashType > 3
// const ERROR_UNKNOWN_VM_VERSION: &str = "Invalid params: the maximum vm version currently supported";

const RFC0032_EPOCH_NUMBER: EpochNumber = 3;

// > * lock script is SECP256K1
// > * lock_vm0 indicates the cycles of running lock script on VM0
// > * lock_vm1 indicates the cycles of running lock script on VM1
// > * type_vm0 indicates the cycles of running type script on VM0
// > * type_vm1 indicates the cycles of running type script on VM1
//
// ```
// ┌─────────────────┬──────────────────┬────────────────────────┬──────────────────────────┐
// │                 │                  │                        │                          │
// │ lock.hash_type  │   type.hash_type │  v2019                 │  v2021                   │
// │                 │                  │                        │                          │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────│
// │                 │                  │                        │                          │
// │ data            │   None           │  Ok(lock_vm0)          │  Ok(lock_vm0)            │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ type            │   None           │  Ok(lock_vm0)          │  Ok(lock_vm1)            │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data1           │   None           │  Incompatible          │  Ok(lock_vm1)            │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data            │   Some(data)     │  Ok(lock_vm0+type_vm0) │  Ok(lock_vm0+type_vm0)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ type            │   Some(data)     │  Ok(lock_vm0+type_vm0) │  Ok(lock_vm1+type_vm0)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data1           │   Some(data)     │  Incompatible          │  Ok(lock_vm1+type_vm0)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data            │   Some(type)     │  Ok(lock_vm0+type_vm0) │  Ok(lock_vm0+type_vm1)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ type            │   Some(type)     │  Ok(lock_vm0+type_vm0) │  Ok(lock_vm1+type_vm1)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data1           │   Some(type)     │  Incompatible          │  Ok(lock_vm1+type_vm1)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data            │   Some(data1)    │  Incompatible          │  Ok(lock_vm0+type_vm1)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ type            │   Some(data1)    │  Incompatible          │  Ok(lock_vm1+type_vm1)   │
// ├─────────────────┼──────────────────┼────────────────────────┼──────────────────────────┤
// │                 │                  │                        │                          │
// │ data1           │   Some(data1)    │  Incompatible          │  Ok(lock_vm1+type_vm1)   │
// │                 │                  │                        │                          │
// └─────────────────┴──────────────────┴────────────────────────┴──────────────────────────┘
// ```
