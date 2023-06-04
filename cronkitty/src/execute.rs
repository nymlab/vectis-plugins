use crate::{
    contract::{CronKittyPlugin, ExecMsg, MANAGER},
    error::ContractError,
};
use cosmwasm_std::{coins, to_binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, WasmMsg};
use vectis_wallet::ProxyExecuteMsg;

pub fn execute(
    cronkitty: &CronKittyPlugin,
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    action_id: u64,
) -> Result<Response, ContractError> {
    let (version, mut msgs, task_hash_stored, auto_refill) =
        cronkitty.actions.load(deps.storage, action_id)?;
    let mgt_addr = cronkitty.query_contract_addr(&deps.as_ref(), &version, MANAGER)?;

    // Make sure it is from the maanger
    if info.sender != mgt_addr {
        return Err(ContractError::Unauthorized);
    }

    // Now: check latest manager taskhash to ensure it is one we created
    // the owner field is already in the task_hash
    let task_info = cronkitty
        .last_task_execution_info
        .query(&deps.querier, mgt_addr.clone())?;

    if let Some(task_hash) = task_hash_stored {
        if task_info.task_hash != task_hash {
            Err(ContractError::UnexpectedCroncatTaskHash)
        } else {
            let owner = deps
                .api
                .addr_humanize(&cronkitty.owner.load(deps.storage)?)?
                .into_string();
            // Once we know it is the taskhash we created, we will
            // 1. Call Proxy PluginExecute to let proxy take action as instructure
            // 2. Check if the task requires auto-refill, if it does, we will refill it to the
            //    expected level

            if let Some(watermark) = auto_refill {
                let current_task_balance_on_croncat = cronkitty
                    .task_balances
                    .query(&deps.querier, mgt_addr, task_hash.as_bytes())?
                    .ok_or(ContractError::UnexpectedCroncatTaskBalance)?;

                if current_task_balance_on_croncat
                    .native_balance
                    .lt(&watermark)
                {
                    // safe unwrap as we already know watermark is gt
                    // current_task_balance_on_croncat
                    let to_refill_amount = watermark
                        .checked_sub(current_task_balance_on_croncat.native_balance)
                        .unwrap();
                    let denom = cronkitty.native_denom.load(deps.storage)?;

                    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: env.contract.address.to_string(),
                        msg: to_binary(&ExecMsg::RefillTask { task_id: action_id })?,
                        funds: coins(to_refill_amount.u128(), denom),
                    }))
                }
            }

            let forward_msgs = CosmosMsg::<_>::Wasm(WasmMsg::Execute {
                contract_addr: owner.clone(),
                msg: to_binary(&ProxyExecuteMsg::PluginExecute { msgs })?,
                funds: vec![],
            });

            Ok(Response::new()
                .add_message(forward_msgs)
                .add_attribute("vectis.cronkitty", "MsgExecute")
                .add_attribute("Proxy", owner)
                .add_attribute("action_id", action_id.to_string()))
        }
    } else {
        Err(ContractError::TaskHashNotFound)
    }
}
