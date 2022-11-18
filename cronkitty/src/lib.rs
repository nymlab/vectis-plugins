mod contract;
mod error;

#[cfg(not(feature = "library"))]
mod entry_points {
    use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response};

    use crate::contract::{ContractExecMsg, ContractQueryMsg, CronKittyPlugin, InstantiateMsg};
    use crate::error::ContractError;

    const CONTRACT: CronKittyPlugin = CronKittyPlugin::new();

    #[entry_point]
    pub fn instantiate(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: InstantiateMsg,
    ) -> Result<Response, ContractError> {
        msg.dispatch(&CONTRACT, (deps, env, info))
    }

    #[entry_point]
    pub fn execute(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ContractExecMsg,
    ) -> Result<Response, ContractError> {
        msg.dispatch(&CONTRACT, (deps, env, info))
    }

    #[entry_point]
    pub fn query(deps: Deps, env: Env, msg: ContractQueryMsg) -> Result<Binary, ContractError> {
        msg.dispatch(&CONTRACT, (deps, env))
    }

    /// reply hooks handles replies from proxy wallet instantiation
    #[cfg_attr(not(feature = "library"), entry_point)]
    pub fn reply(deps: DepsMut, _env: Env, reply: Reply) -> Result<Response, ContractError> {
        // NOTE: Error returned in `reply` is equivalent to contract error, all states revert,
        // specifically, the TOTAL_CREATED incremented in `create_wallet` will revert

        let expected_id = CONTRACT.action_id.load(deps.storage)?;

        if reply.id == expected_id {
            CONTRACT.action_id.update(deps.storage, |id| {
                id.checked_add(1).ok_or(ContractError::Overflow)
            })?;
            Ok(Response::new())
        } else {
            Err(ContractError::InvalidReplyId)
        }
    }
}

#[cfg(not(feature = "library"))]
pub use crate::entry_points::*;
