use avida_verifier::{
    state::plugin::{SELF_ISSUED_CRED_DEF, VECTIS_ACCOUNT},
    types::WCredentialPubKey,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use cw2::set_contract_version;
use thiserror::Error;

// version info for migration info
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("Identity Plugin Inst Failed")]
    IdentityPluginInstFailed,
    #[error("StdError {0}")]
    Std(#[from] StdError),
    #[error("Not implemented")]
    NotImplemented,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub cred_def: WCredentialPubKey,
}

#[cw_serde]
pub struct ExecuteMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(WCredentialPubKey)]
    CredentialPubKey,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    VECTIS_ACCOUNT.save(deps.storage, &info.sender)?;
    SELF_ISSUED_CRED_DEF.save(deps.storage, &msg.cred_def)?;
    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Err(ContractError::NotImplemented)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::CredentialPubKey => to_binary(&query_cred_pub_key(deps)?),
    }
}

fn query_cred_pub_key(deps: Deps) -> StdResult<WCredentialPubKey> {
    SELF_ISSUED_CRED_DEF.load(deps.storage)
}
