pub mod contract;
pub mod error;

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

    use crate::contract::{ContractExecMsg, ContractQueryMsg, CronKittyPlugin, InstantiateMsg};
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

    /// reply hooks handles replies from proxy wallet instantiation
    #[entry_point]
    pub fn reply(deps: DepsMut, _env: Env, reply: Reply) -> Result<Response, ContractError> {
        // NOTE: Error returned in `reply` is equivalent to contract error, all states revert,
        // specifically, the TOTAL_CREATED incremented in `create_wallet` will revert

        if let (_, _, _, Some(task_hash)) = CONTRACT.actions.load(deps.storage, reply.id)? {
            // This means task_hash was stored, i.e. replied from remove_task
            CONTRACT.actions.remove(deps.storage, reply.id);
            Ok(Response::new().add_event(
                Event::new("vectis.cronkitty.v1.ReplyRemoveTask")
                    .add_attribute("Task ID", reply.id.to_string())
                    .add_attribute("Task Hash", task_hash),
            ))
        } else {
            let expected_id = CONTRACT.next_action_id.load(deps.storage)?;
            if reply.id == expected_id {
                // only reply_on_success
                let reply_data = parse_reply_execute_data(reply)?
                    .data
                    .ok_or(ContractError::UnexpectedCroncatTaskReply)?;
                let task_exec_info: TaskExecutionInfo = from_binary(&reply_data)?;

                // TODO: do we want to store other things?
                let task_hash = task_exec_info.task_hash;

                CONTRACT.actions.update(
                    deps.storage,
                    expected_id,
                    |t| -> Result<
                        ([u8; 2], [u8; 2], Vec<CosmosMsg>, Option<String>),
                        ContractError,
                    > {
                        let mut task = t.ok_or(ContractError::TaskHashNotFound)?;
                        task.3 = Some(task_hash);
                        Ok(task)
                    },
                )?;

                CONTRACT.next_action_id.update(deps.storage, |id| {
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
