pub mod contract;
pub mod error;
#[cfg(any(test, features = "tests"))]
pub mod multitest;

#[cfg(test)]
mod tests;

#[cfg(not(feature = "library"))]
mod entry_points {
    use cosmwasm_std::{
        entry_point, Binary, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo, Reply, Response,
    };

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

        if let (_, Some(task_hash)) = CONTRACT.actions.load(deps.storage, reply.id)? {
            // This means task_hash was stored, i.e. replied from remove_task
            CONTRACT.actions.remove(deps.storage, reply.id);
            Ok(Response::new().add_event(
                Event::new("vectis.cronkitty.v1.ReplyRemoveTask")
                    .add_attribute("Task ID", reply.id.to_string())
                    .add_attribute("Task Hash", task_hash),
            ))
        } else {
            let expected_id = CONTRACT.action_id.load(deps.storage)?;
            if reply.id == expected_id {
                // only reply_on_success
                let r = reply.result.unwrap();
                let task_hash = r
                    .events
                    .iter()
                    .find(|e| e.ty == "wasm")
                    .ok_or(ContractError::ExpectedEventNotFound)?
                    .attributes
                    .iter()
                    .find(|attr| attr.key == "task_hash")
                    .ok_or(ContractError::TaskHashNotFound)?;

                CONTRACT.actions.update(
                    deps.storage,
                    expected_id,
                    |t| -> Result<(Vec<CosmosMsg>, Option<String>), ContractError> {
                        let task = t.ok_or(ContractError::TaskHashNotFound)?;
                        Ok((task.0, Some(task_hash.value.clone())))
                    },
                )?;

                CONTRACT.action_id.update(deps.storage, |id| {
                    id.checked_add(1).ok_or(ContractError::Overflow)
                })?;

                Ok(Response::new())
            } else {
                Err(ContractError::InvalidReplyId)
            }
        }
    }
}

#[cfg(not(feature = "library"))]
pub use crate::entry_points::*;
