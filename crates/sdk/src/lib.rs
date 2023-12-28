// -------------------------------- all targets --------------------------------

mod deps;
mod item;
mod map;
mod map_key;
mod result;
mod serde;
mod testing;
mod traits;

pub use {
    crate::{
        deps::ExecuteCtx,
        item::Item,
        map::Map,
        map_key::{MapKey, RawKey},
        result::{ContractResult, Response},
        serde::{from_json, to_json},
        testing::MockStorage,
        traits::Storage,
    },
    cw_sdk_derive::{cw_serde, entry_point},
};

// ---------------------------- wasm32 target only -----------------------------

#[cfg(target_arch = "wasm32")]
mod exports;
#[cfg(target_arch = "wasm32")]
mod imports;
#[cfg(target_arch = "wasm32")]
mod memory;

#[cfg(target_arch = "wasm32")]
pub use crate::{exports::do_execute, imports::ExternalStorage, memory::Region};

// -------------------------------- re-exports ---------------------------------

pub mod __private {
    pub use serde;
}