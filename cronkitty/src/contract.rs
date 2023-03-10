use crate::error::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, to_binary, CanonicalAddr, CosmosMsg, Deps, DepsMut, Empty, Env, Event,
    MessageInfo, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use croncat_sdk_manager::msg::ManagerExecuteMsg as CCManagerExecMsg;
use croncat_sdk_tasks::{
    msg::TasksExecuteMsg as CCTaskExecMsg,
    types::{Action, TaskRequest},
};
use cw2::set_contract_version;
use cw_storage_plus::{Item, Map};
use sylvia::contract;
use vectis_wallet::ProxyExecuteMsg;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
pub struct CronKittyActionResp {
    pub msgs: Vec<CosmosMsg>,
    pub task_hash: Option<String>,
}

pub struct CronKittyPlugin<'a> {
    pub actions: Map<'a, u64, (Vec<CosmosMsg>, Option<String>)>,
    pub owner: Item<'a, CanonicalAddr>,
    pub action_id: Item<'a, u64>,
    pub croncat_manager: Item<'a, CanonicalAddr>,
    pub croncat_tasks: Item<'a, CanonicalAddr>,
}

#[contract]
impl CronKittyPlugin<'_> {
    pub const fn new() -> Self {
        Self {
            actions: Map::new("actions"),
            owner: Item::new("owner"),
            action_id: Item::new("id"),
            // Croncat Manager calls for execute
            croncat_manager: Item::new("croncat-manager"),
            // Croncat Tasks handles creating
            croncat_tasks: Item::new("croncat-tasks"),
        }
    }

    #[msg(instantiate)]
    pub fn instantiate(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        croncat_manager_addr: String,
        croncat_tasks_addr: String,
    ) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        self.owner.save(
            deps.storage,
            &deps.api.addr_canonicalize(info.sender.as_str())?,
        )?;
        let croncat_manager = deps
            .api
            .addr_canonicalize(deps.api.addr_validate(&croncat_manager_addr)?.as_str())?;
        let croncat_tasks = deps
            .api
            .addr_canonicalize(deps.api.addr_validate(&croncat_tasks_addr)?.as_str())?;
        self.croncat_manager.save(deps.storage, &croncat_manager)?;
        self.croncat_tasks.save(deps.storage, &croncat_tasks)?;
        self.action_id.save(deps.storage, &0)?;
        Ok(Response::new())
    }

    #[msg(exec)]
    pub fn execute(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        action_id: u64,
    ) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        if info.sender
            != deps
                .api
                .addr_humanize(&self.croncat_manager.load(deps.storage)?)?
        {
            Err(ContractError::Unauthorized)
        } else {
            let taskx = self.actions.load(deps.storage, action_id)?;
            let owner = deps
                .api
                .addr_humanize(&self.owner.load(deps.storage)?)?
                .into_string();
            let msg = CosmosMsg::<_>::Wasm(WasmMsg::Execute {
                contract_addr: owner.clone(),
                msg: to_binary(&ProxyExecuteMsg::PluginExecute { msgs: taskx.0 })?,
                funds: vec![],
            });
            let event = Event::new("vectis.cronkitty.v1.MsgExecute").add_attribute("Proxy", owner);
            Ok(Response::new().add_event(event).add_message(msg))
        }
    }

    #[msg(exec)]
    fn create_task(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        mut task: TaskRequest,
    ) -> Result<Response, ContractError> {
        let (deps, env, info) = ctx;

        // only the owner (proxy) can create task
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else {
            // The id for croncat to call back
            let id = self.action_id.load(deps.storage)?;
            self.actions.save(
                deps.storage,
                id,
                &(task.actions.iter().cloned().map(|a| a.msg).collect(), None),
            )?;

            // make sure forward all gas
            let gas_limit = task.actions.iter().try_fold(0u64, |acc, a| {
                acc.checked_add(a.gas_limit.unwrap_or(0))
                    .ok_or(ContractError::Overflow)
            })?;
            let denom = deps.querier.query_bonded_denom()?;
            ensure!(
                info.funds
                    .iter()
                    .find(|c| c.denom == denom)
                    .unwrap_or(&coin(0, denom))
                    .amount
                    >= Uint128::from(gas_limit),
                ContractError::NotEnoughFundsForGas
            );

            let gas_limit = if gas_limit == 0 {
                None
            } else {
                Some(gas_limit)
            };

            let action = Action {
                msg: CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecMsg::Execute { action_id: id })?,
                    funds: vec![],
                }),
                gas_limit,
            };

            // We forward all the other params (so we can contribute to / use to frontend code from
            // croncat)
            // The Action called is to call this plugin at the given intervals
            task.actions = vec![action];
            task.cw20 = None;

            let msg = SubMsg::reply_on_success(
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: deps
                        .api
                        .addr_humanize(&self.croncat_tasks.load(deps.storage)?)?
                        .to_string(),
                    msg: to_binary(&CCTaskExecMsg::CreateTask {
                        task: Box::new(task),
                    })?,
                    // TODO: https://github.com/CronCats/cw-croncat/issues/204
                    funds: info.funds,
                }),
                id,
            );

            Ok(Response::new().add_submessage(msg))
        }
    }

    #[msg(exec)]
    pub fn remove_task(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        task_id: u64,
    ) -> Result<Response, ContractError> {
        let (deps, _env, info) = ctx;

        // only the owner (proxy) can create task
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else {
            // call croncat to remove task
            if let (_, Some(task_hash)) = self.actions.load(deps.storage, task_id)? {
                let msg = SubMsg::reply_on_success(
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: deps
                            .api
                            .addr_humanize(&self.croncat_tasks.load(deps.storage)?)?
                            .to_string(),
                        msg: to_binary(&CCTaskExecMsg::RemoveTask { task_hash })?,
                        funds: vec![],
                    }),
                    task_id,
                );
                Ok(Response::new().add_submessage(msg))
            } else {
                Err(ContractError::TaskHashNotFound)
            }
        }
    }

    #[msg(exec)]
    pub fn refill_task(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        task_id: u64,
    ) -> Result<Response, ContractError> {
        let (deps, _env, info) = ctx;

        if info.funds.is_empty() {
            return Err(ContractError::EmptyFunds);
        }

        // only the owner (proxy) can create task
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else {
            // call croncat to remove task
            if let (_, Some(task_hash)) = self.actions.load(deps.storage, task_id)? {
                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: deps
                        .api
                        .addr_humanize(&self.croncat_manager.load(deps.storage)?)?
                        .to_string(),
                    msg: to_binary(&CCManagerExecMsg::RefillTaskBalance { task_hash })?,
                    funds: info.funds,
                });
                Ok(Response::new().add_message(msg))
            } else {
                Err(ContractError::TaskHashNotFound)
            }
        }
    }

    #[msg(query)]
    pub fn action_id(&self, ctx: (Deps, Env)) -> StdResult<u64> {
        let (deps, _) = ctx;
        self.action_id.load(deps.storage)
    }

    // These are the id that stores the actual cosmos messages
    #[msg(query)]
    pub fn action(&self, ctx: (Deps, Env), action_id: u64) -> StdResult<CronKittyActionResp> {
        let (deps, _) = ctx;
        let (msgs, task_hash) = self.actions.load(deps.storage, action_id)?;
        Ok(CronKittyActionResp { msgs, task_hash })
    }

    #[msg(migrate)]
    fn migrate(&self, _ctx: (DepsMut, Env)) -> Result<Response, ContractError> {
        // Not used but required for impl for multitest
        Ok(Response::default())
    }
}
