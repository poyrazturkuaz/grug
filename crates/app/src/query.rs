use {
    crate::{
        create_vm_instance, load_program, AppError, AppResult, PrefixStore, Vm, ACCOUNTS, CHAIN_ID,
        CODES, CONFIG, CONTRACT_NAMESPACE, LAST_FINALIZED_BLOCK,
    },
    grug_storage::Bound,
    grug_types::{
        AccountResponse, Addr, BankQueryMsg, BankQueryResponse, Binary, BlockInfo, Coin, Coins,
        Context, Hash, InfoResponse, Json, Order, StdResult, Storage, WasmRawResponse,
        WasmSmartResponse,
    },
};

const DEFAULT_PAGE_LIMIT: u32 = 30;

pub fn query_info(store: &dyn Storage) -> AppResult<InfoResponse> {
    Ok(InfoResponse {
        chain_id:             CHAIN_ID.load(store)?,
        config:               CONFIG.load(store)?,
        last_finalized_block: LAST_FINALIZED_BLOCK.load(store)?,
    })
}

pub fn query_balance<VM>(
    store:   Box<dyn Storage>,
    block:   &BlockInfo,
    address: Addr,
    denom:   String,
) -> AppResult<Coin>
where
    VM: Vm + 'static,
    AppError: From<VM::Error>,
{
    _query_bank::<VM>(store, block, &BankQueryMsg::Balance { address, denom })
        .map(|res| res.as_balance())
}

pub fn query_balances<VM>(
    store:       Box<dyn Storage>,
    block:       &BlockInfo,
    address:     Addr,
    start_after: Option<String>,
    limit:       Option<u32>,
) -> AppResult<Coins>
where
    VM: Vm + 'static,
    AppError: From<VM::Error>,
{
    _query_bank::<VM>(store, block, &BankQueryMsg::Balances { address, start_after, limit })
        .map(|res| res.as_balances())
}

pub fn query_supply<VM>(
    store: Box<dyn Storage>,
    block: &BlockInfo,
    denom: String,
) -> AppResult<Coin>
where
    VM: Vm + 'static,
    AppError: From<VM::Error>,
{
    _query_bank::<VM>(store, block, &BankQueryMsg::Supply { denom })
        .map(|res| res.as_supply())
}

pub fn query_supplies<VM>(
    store:       Box<dyn Storage>,
    block:       &BlockInfo,
    start_after: Option<String>,
    limit:       Option<u32>,
) -> AppResult<Coins>
where
    VM: Vm + 'static,
    AppError: From<VM::Error>,
{
    _query_bank::<VM>(store, block, &BankQueryMsg::Supplies { start_after, limit })
        .map(|res| res.as_supplies())
}

pub fn _query_bank<VM>(
    store: Box<dyn Storage>,
    block: &BlockInfo,
    msg:   &BankQueryMsg,
) -> AppResult<BankQueryResponse>
where
    VM: Vm + 'static,
    AppError: From<VM::Error>,
{
    // load program code
    let chain_id = CHAIN_ID.load(&store)?;
    let cfg = CONFIG.load(&store)?;
    let account = ACCOUNTS.load(&store, &cfg.bank)?;

    // create VM instance
    let program = load_program::<VM>(&store, &account.code_hash)?;
    let instance = create_vm_instance::<VM>(store.clone(), block.clone(), &cfg.bank, program)?;

    // call query
    let ctx = Context {
        chain_id,
        block_height:    block.height,
        block_timestamp: block.timestamp,
        block_hash:      block.hash.clone(),
        contract:        cfg.bank,
        sender:          None,
        funds:           None,
        simulate:        None,
    };
    instance.call_bank_query(&ctx, msg)?.into_std_result().map_err(Into::into)
}

pub fn query_code(store: &dyn Storage, hash: Hash) -> AppResult<Binary> {
    Ok(CODES.load(store, &hash)?.into())
}

pub fn query_codes(
    store:       &dyn Storage,
    start_after: Option<Hash>,
    limit:       Option<u32>,
) -> AppResult<Vec<Hash>> {
    let start = start_after.as_ref().map(Bound::exclusive);
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);

    CODES
        .keys(store, start, None, Order::Ascending)
        .take(limit as usize)
        .collect::<StdResult<Vec<_>>>()
        .map_err(Into::into)
}

pub fn query_account(store: &dyn Storage, address: Addr) -> AppResult<AccountResponse> {
    let account = ACCOUNTS.load(store, &address)?;
    Ok(AccountResponse {
        address,
        code_hash: account.code_hash,
        admin:     account.admin,
    })
}

pub fn query_accounts(
    store:       &dyn Storage,
    start_after: Option<Addr>,
    limit:       Option<u32>,
) -> AppResult<Vec<AccountResponse>> {
    let start = start_after.as_ref().map(Bound::exclusive);
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);

    ACCOUNTS
        .range(store, start, None, Order::Ascending)
        .take(limit as usize)
        .map(|item| {
            let (address, account) = item?;
            Ok(AccountResponse {
                address,
                code_hash: account.code_hash,
                admin:     account.admin,
            })
        })
        .collect()
}

pub fn query_wasm_raw(
    store:    Box<dyn Storage>,
    contract: Addr,
    key:      Binary,
) -> AppResult<WasmRawResponse> {
    let substore = PrefixStore::new(store, &[CONTRACT_NAMESPACE, &contract]);
    let value = substore.read(&key);
    Ok(WasmRawResponse {
        contract,
        key,
        value: value.map(Binary::from),
    })
}

pub fn query_wasm_smart<VM>(
    store:    Box<dyn Storage>,
    block:    &BlockInfo,
    contract: Addr,
    msg:      Json,
) -> AppResult<WasmSmartResponse>
where
    VM: Vm + 'static,
    AppError: From<VM::Error>,
{
    let chain_id = CHAIN_ID.load(&store)?;
    let account = ACCOUNTS.load(&store, &contract)?;

    let program = load_program::<VM>(&store, &account.code_hash)?;
    let instance = create_vm_instance::<VM>(store, block.clone(), &contract, program)?;

    // call query
    let ctx = Context {
        chain_id,
        contract,
        block_height:    block.height,
        block_timestamp: block.timestamp,
        block_hash:      block.hash.clone(),
        sender:          None,
        funds:           None,
        simulate:        None,
    };
    let data = instance.call_query(&ctx, &msg)?.into_std_result()?;

    Ok(WasmSmartResponse {
        contract: ctx.contract,
        data,
    })
}
