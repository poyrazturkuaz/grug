use {
    crate::{Addr, Binary, Coins, Hash},
    serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Tx {
    pub sender:     Addr,
    pub msgs:       Vec<Message>,
    pub credential: Binary,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Message {
    /// Send coins to the given recipient address.
    ///
    /// Note that we don't assert the recipient is an account that exists, only
    /// that it's a valid 32-byte hex string. The sender is reponsible to make
    /// sure to put the correct address.
    Transfer {
        to:    Addr,
        coins: Coins,
    },
    /// Upload a Wasm binary code and store it in the chain's state.
    StoreCode {
        wasm_byte_code: Binary,
    },
    /// Register a new account.
    Instantiate {
        code_hash: Hash,
        msg:       Binary,
        salt:      Binary,
        funds:     Coins,
        admin:     Option<Addr>,
    },
    /// Execute the contract.
    Execute {
        contract: Addr,
        msg:      Binary,
        funds:    Coins,
    },
    // TODO: migrate
}
