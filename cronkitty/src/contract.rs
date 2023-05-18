use crate::error::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, to_binary, Addr, CanonicalAddr, CosmosMsg, Deps, DepsMut, Empty, Env, Event,
    MessageInfo, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use croncat_sdk_factory::state::CONTRACT_ADDRS;
use croncat_sdk_manager::msg::ManagerExecuteMsg as CCManagerExecMsg;
use croncat_sdk_tasks::{
    msg::TasksExecuteMsg as CCTaskExecMsg,
    types::{Action, TaskExecutionInfo, TaskRequest},
};
use cw2::set_contract_version;
use cw_storage_plus::{Item, Map};
use sylvia::contract;
use vectis_wallet::ProxyExecuteMsg;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const TASK: &str = "tasks";
const MANAGER: &str = "manager";

#[cw_serde]
pub struct CronKittyActionResp {
    pub msgs: Vec<CosmosMsg>,
    pub task_hash: Option<String>,
    pub task_addr: Addr,
    pub manager_addr: Addr,
}

pub struct CronKittyPlugin<'a> {
    // Pending get task hash
    // Map <action_id, (task_version, mg_version, msg_for_proxy_to_exec, task_hash_on_croncat )>
    pub actions: Map<'a, u64, ([u8; 2], [u8; 2], Vec<CosmosMsg>, Option<String>)>,
    // This is only used when waiting for reply from croncat on task_creation completion
    // (action_id, task_version, mg_version, msg_for_proxy_to_exec)
    //pub pending_action: Item<'a, (u64, [u8; 2], [u8; 2], Vec<CosmosMsg>)>,
    pub owner: Item<'a, CanonicalAddr>,
    pub next_action_id: Item<'a, u64>,
    pub croncat_factory: Item<'a, CanonicalAddr>,
    // Latest contract name to the version
    // perhaps can also move this to croncat_factory_sdk::state?
    pub latest_versions: Map<'a, &'a str, [u8; 2]>,
    pub last_task_execution_info: Item<'a, TaskExecutionInfo>,
}

#[contract]
impl CronKittyPlugin<'_> {
    pub const fn new() -> Self {
        Self {
            actions: Map::new("actions"),
            owner: Item::new("owner"),
            next_action_id: Item::new("id"),
            croncat_factory: Item::new("croncat-manager"),
            latest_versions: Map::new("latest_versions"),
            last_task_execution_info: Item::new("last_task_execution_info"),
        }
    }

    #[msg(instantiate)]
    pub fn instantiate(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        croncat_factory_addr: String,
        vectis_account_addr: String,
    ) -> Result<Response, ContractError> {
        let (deps, _, _) = ctx;
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        self.owner.save(
            deps.storage,
            &deps.api.addr_canonicalize(&vectis_account_addr)?,
        )?;

        // Validate CronCat Factory address
        let croncat_factory = deps
            .api
            .addr_canonicalize(deps.api.addr_validate(&croncat_factory_addr)?.as_str())?;

        self.croncat_factory.save(deps.storage, &croncat_factory)?;
        self.next_action_id.save(deps.storage, &0)?;

        Ok(Response::new())
    }

    #[msg(exec)]
    pub fn execute(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        action_id: u64,
    ) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        let (_, mgr_version, msgs, _) = self.actions.load(deps.storage, action_id)?;
        let mgt_addr = self.query_contract_addr(&deps.as_ref(), &mgr_version, MANAGER)?;

        // MAke sure it is from the maanger
        if info.sender != mgt_addr {
            return Err(ContractError::Unauthorized);
        }

        // Now: check latest manager taskhash to ensure it is one we created
        let task_info = self
            .last_task_execution_info
            .query(&deps.querier, mgt_addr)?;
        let stored_task_info = self.actions.load(deps.storage, action_id)?;
        if let Some(task_hash) = stored_task_info.3 {
            if task_info.task_hash != task_hash {
                return Err(ContractError::UnexpectedCroncatTaskHash);
            } else {
                let owner = deps
                    .api
                    .addr_humanize(&self.owner.load(deps.storage)?)?
                    .into_string();
                let msg = CosmosMsg::<_>::Wasm(WasmMsg::Execute {
                    contract_addr: owner.clone(),
                    msg: to_binary(&ProxyExecuteMsg::PluginExecute { msgs })?,
                    funds: vec![],
                });
                let event =
                    Event::new("vectis.cronkitty.v1.MsgExecute").add_attribute("Proxy", owner);
                Ok(Response::new().add_event(event).add_message(msg))
            }
        } else {
            return Err(ContractError::TaskHashNotFound);
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
            let task_contract_version =
                self.query_latest_version_croncat_contract(&deps.as_ref(), TASK)?;
            let mgr_contract_version =
                self.query_latest_version_croncat_contract(&deps.as_ref(), MANAGER)?;
            let task_contract_addr =
                self.query_contract_addr(&deps.as_ref(), &task_contract_version, TASK)?;
            // The id for croncat to call back
            let id = self.next_action_id.load(deps.storage)?;
            self.actions.save(
                deps.storage,
                id,
                &(
                    task_contract_version,
                    mgr_contract_version,
                    task.actions.iter().cloned().map(|a| a.msg).collect(),
                    None,
                ),
            )?;

            // make sure forward all gas
            // wasm gas_limit is provided by simulation in the frontend
            // We either do something similar to `validate_msg_calculate_usage`
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
                    contract_addr: task_contract_addr.to_string(),
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
            if let (task_contract_version, _, _, Some(task_hash)) =
                self.actions.load(deps.storage, task_id)?
            {
                let task =
                    self.query_contract_addr(&deps.as_ref(), &task_contract_version, TASK)?;
                let msg = SubMsg::reply_on_success(
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: task.to_string(),
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
            // call croncat to refill task
            if let (_, mgr_contract_version, _, Some(task_hash)) =
                self.actions.load(deps.storage, task_id)?
            {
                let manager =
                    self.query_contract_addr(&deps.as_ref(), &mgr_contract_version, MANAGER)?;
                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: manager.to_string(),
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
        self.next_action_id.load(deps.storage)
    }

    // These are the id that stores the actual cosmos messages
    #[msg(query)]
    pub fn action(
        &self,
        ctx: (Deps, Env),
        action_id: u64,
    ) -> Result<CronKittyActionResp, ContractError> {
        let (deps, _) = ctx;
        let (task_version, mgr_version, msgs, task_hash) =
            self.actions.load(deps.storage, action_id)?;
        let task_addr = self.query_contract_addr(&deps, &task_version, TASK)?;
        let manager_addr = self.query_contract_addr(&deps, &mgr_version, MANAGER)?;
        Ok(CronKittyActionResp {
            msgs,
            task_hash,
            task_addr,
            manager_addr,
        })
    }

    #[msg(migrate)]
    fn migrate(&self, _ctx: (DepsMut, Env)) -> Result<Response, ContractError> {
        // Not used but required for impl for multitest
        Ok(Response::default())
    }

    fn query_latest_version_croncat_contract(
        &self,
        deps: &Deps,
        name: &str,
    ) -> Result<[u8; 2], ContractError> {
        let cc_factory = deps
            .api
            .addr_humanize(&self.croncat_factory.load(deps.storage)?)?;

        self.latest_versions
            .query(&deps.querier, cc_factory, &name)
            .transpose()
            .ok_or_else(|| ContractError::NoCronCatVersion {
                name: name.to_string(),
            })?
            .map_err(|e| e.into())
    }

    /// Takes a CronCat contract name, queries the factory for the latest contract address.
    /// Returns a result with the latest version and the addr, or an error.
    fn query_contract_addr(
        &self,
        deps: &Deps,
        version: &[u8; 2],
        name: &str,
    ) -> Result<Addr, ContractError> {
        CONTRACT_ADDRS
            .query(
                &deps.querier,
                deps.api
                    .addr_humanize(&self.croncat_factory.load(deps.storage)?)?,
                (name, version),
            )?
            .ok_or_else(|| ContractError::NoCronCatContract {
                name: name.to_string(),
            })
    }
}
