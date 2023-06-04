pub mod contract;
pub mod error;
pub mod execute;

#[cfg(test)]
pub mod multitest;
#[cfg(test)]
mod tests;

mod entry_points {
    use cosmwasm_std::{
        entry_point, from_binary, Binary, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo, Reply,
        Response,
    };
    use cw_utils::parse_reply_execute_data;

    use crate::contract::{
        ContractExecMsg, ContractQueryMsg, CronKittyPlugin, CronkittyActionRef, InstantiateMsg,
    };
    use crate::error::ContractError;
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
        if let (_, _, Some(task_hash), _) = CONTRACT.actions.load(deps.storage, reply.id)? {
            // This means task_hash was stored, i.e. replied from remove_task
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

            Ok(res.add_event(
                Event::new("vectis.cronkitty.v1.ReplyRemoveTask")
                    .add_attribute("Task ID", reply.id.to_string())
                    .add_attribute("Task Hash", task_hash),
            ))
        } else {
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
                    |t| -> Result<CronkittyActionRef, ContractError> {
                        let mut task = t.ok_or(ContractError::TaskNotFound)?;
                        task.2 = Some(task_hash.clone());
                        Ok(task)
                    },
                )?;

                CONTRACT.next_action_id.update(deps.storage, |id| {
                    id.checked_add(1).ok_or(ContractError::Overflow)
                })?;

                Ok(Response::new().add_event(
                    Event::new("vectis.cronkitty.v1.ReplyCreateTask")
                        .add_attribute("Task ID", reply_id.to_string())
                        .add_attribute("Task Hash", task_hash),
                ))
            } else {
                Err(ContractError::InvalidReplyId)
            }
        }
    }
}

#[cfg(not(feature = "library"))]
pub use crate::entry_points::*;
