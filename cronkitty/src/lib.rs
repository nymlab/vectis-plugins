pub mod contract;
pub mod error;
pub mod execute;
pub mod types;

#[cfg(test)]
pub mod multitest;
#[cfg(test)]
mod tests;

pub const ACTION_ERROR_REPLY_ID: u64 = u64::MAX;
mod entry_points {
    use cosmwasm_std::{
        entry_point, from_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply,
        Response,
    };
    use cw_utils::parse_reply_execute_data;

    use crate::{
        contract::{ContractExecMsg, ContractQueryMsg, CronKittyPlugin, InstantiateMsg},
        error::ContractError,
        types::ActionRef,
        ACTION_ERROR_REPLY_ID,
    };
    use croncat_sdk_tasks::types::TaskExecutionInfo;

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

    #[entry_point]
    pub fn reply(deps: DepsMut, env: Env, reply: Reply) -> Result<Response, ContractError> {
        // This is going to be an error because it is `reply_on_error`
        if reply.id == ACTION_ERROR_REPLY_ID {
            // We update failure refill accounting
            let action_id = CONTRACT.exec_action_id.load(deps.storage)?;
            CONTRACT.actions.update(
                deps.storage,
                action_id,
                |a| -> Result<ActionRef, ContractError> {
                    let action_ref = a.ok_or(ContractError::TaskNotFound)?;
                    action_ref.failed_action()
                },
            )?;
            let err = parse_reply_execute_data(reply).unwrap_err();
            return Ok(
                Response::new().add_attribute("Vectis PluginExecute Error", format! {"{err}"})
            );
        }
        let action = CONTRACT.actions.load(deps.storage, reply.id)?;

        // if reply is not an action error, it will be from either
        // 1. task creation - previously not set task_hash
        // 2. task deletion - have previous task_hash

        // Task deletion
        if let Some(task_hash) = &action.task_hash {
            CONTRACT.actions.remove(deps.storage, reply.id);

            // CronCat would have refunded cronkitty
            let balances = deps
                .querier
                .query_all_balances(env.contract.address.as_str())?;
            let owner = CONTRACT.owner.load(deps.storage)?;
            let res = if !balances.is_empty() {
                let msg = CosmosMsg::Bank(cosmwasm_std::BankMsg::Send {
                    to_address: deps.api.addr_humanize(&owner)?.to_string(),
                    amount: balances,
                });
                Response::new().add_message(msg)
            } else {
                Response::new()
            };

            Ok(res
                .add_attribute("vectis.cronkitty.v1", "task_deletion")
                .add_attribute("Task ID", reply.id.to_string())
                .add_attribute("Task Hash", task_hash))
        } else {
            // Task Creation
            let expected_id = CONTRACT.next_action_id.load(deps.storage)?;
            let reply_id = reply.id;
            if reply_id == expected_id {
                let reply_data = parse_reply_execute_data(reply)?
                    .data
                    .ok_or(ContractError::UnexpectedCroncatTaskReply)?;
                let task_exec_info: TaskExecutionInfo = from_binary(&reply_data)?;
                let task_hash = task_exec_info.task_hash;
                CONTRACT.actions.update(
                    deps.storage,
                    expected_id,
                    |t| -> Result<ActionRef, ContractError> {
                        let mut task = t.ok_or(ContractError::TaskNotFound)?;
                        task.task_hash = Some(task_hash.clone());
                        Ok(task)
                    },
                )?;

                CONTRACT.next_action_id.update(deps.storage, |id| {
                    id.checked_add(1).ok_or(ContractError::Overflow)
                })?;

                Ok(Response::new()
                    .add_attribute("vectis.cronkitty.v1", "task_creation")
                    .add_attribute("Task ID", reply_id.to_string())
                    .add_attribute("Task Hash", task_hash))
            } else {
                Err(ContractError::InvalidReplyId)
            }
        }
    }
}

#[cfg(not(feature = "library"))]
pub use crate::entry_points::*;
