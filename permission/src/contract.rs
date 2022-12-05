use crate::error::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, Binary, BlockInfo, CanonicalAddr, Coin, CosmosMsg, DepsMut, Empty, Env,
    MessageInfo, Response, SubMsg, WasmMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::{Item, Map};
use serde::{Deserialize, Serialize};
use sylvia::contract;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct PermissionPlugin<'a> {
    pub(crate) grantees: Map<'a, Addr, Permission>,
    pub(crate) owner: Item<'a, Addr>,
}

#[contract]
impl PermissionPlugin<'_> {
    pub const fn new() -> Self {
        Self {
            grantees: Map::new("grantees"),
            owner: Item::new("owner"),
        }
    }

    #[msg(instantiate)]
    pub fn instantiate(&self, ctx: (DepsMut, Env, MessageInfo)) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        self.owner.save(deps.storage, &info.sender)?;
        Ok(Response::new())
    }

    #[msg(exec)]
    fn grantee_executes(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        msgs: Vec<CosmosMsg>,
    ) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        let permission = self.grantees.load(deps.storage, info.sender)?;
        let res = Response::new();
        self.ensure_permission(&permission, &msgs)?;
        Ok(res.add_messages(msgs))
    }

    #[msg(exec)]
    fn grant_permission(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        plugin: String,
        permission: Permission,
    ) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        if info.sender != self.owner.load(deps.storage)? {
            return Err(ContractError::Unauthorized {});
        };

        self.grantees
            .save(deps.storage, deps.api.addr_validate(&plugin)?, &permission)?;

        Ok(Response::new())
    }

    fn ensure_permission(
        &self,
        permission: &Permission,
        _msgs: &Vec<CosmosMsg>,
    ) -> Result<(), ContractError> {
		// TODO: obviously not all grantees have AllTx
        match permission {
            Permission::AllTx() => Ok(()),
            _ => Err(ContractError::Unauthorized {}),
        }
    }
}

//pub struct PermissionBinary<T: Serialize + std::fmt::Debug> {
//    msgs: Vec<T>,
//}
//
///// for a wasm msg
///// ```
///// CosmosMsg::Wasm(WasmMsg::Execute{
///// contract_addr: <some-addr>,
///// msg: Binary, //
///// funds: Vec<Coin>})
///// ```
///// We want to match binary?
//pub struct CosmosMsgPermissions {}

/// Permission
/// The calling address must have already been added to Grantees.
#[cw_serde]
pub enum Permission {
	/// The grantee is trusted and can execute any tx on the Proxy
	/// Typically this will be plugin code instead of user accounts
	/// although that can also happen if proxy controller wishes
    AllTx(),
    Msg(Vec<Binary>),
    Allowance(Option<Vec<Coin>>),
    // TODO need to check BlockInfo
    Intervals(),
}

// impl Permission {
//     fn match_multi(
//         &self,
//         // set permission interval, executable time
//         _block: &BlockInfo,
//         // set permission sender and funds required
//         _info: &MessageInfo,
//         // set types of messages
//         _msgs: &Vec<CosmosMsg>,
//     ) -> Result<(), ContractError> {
//         Ok(())
//     }
}
