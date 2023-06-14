use crate::{
    error::ContractError,
    execute,
    types::{ActionRef, AutoRefill, CronKittyActionResp},
};
use cosmwasm_std::{
    to_binary, Addr, CanonicalAddr, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdResult, SubMsg, WasmMsg,
};
use croncat_sdk_factory::state::CONTRACT_ADDRS;
use croncat_sdk_manager::{
    msg::{ManagerExecuteMsg as CCManagerExecMsg, ManagerQueryMsg as CCManagerQueryMsg},
    types::{Config as CCConfig, TaskBalance},
};
use croncat_sdk_tasks::{
    msg::TasksExecuteMsg as CCTaskExecMsg,
    types::{Action, TaskExecutionInfo, TaskRequest},
};
use cw2::set_contract_version;
use cw_storage_plus::{Item, Map};
use sylvia::contract;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const TASK: &str = "tasks";
pub(crate) const MANAGER: &str = "manager";

pub struct CronKittyPlugin<'a> {
    /// All the actions stored by Proxy
    pub actions: Map<'a, u64, ActionRef>,
    /// Owner of this contract, aka - Proxy
    pub owner: Item<'a, CanonicalAddr>,
    /// Increments of unique id for actions
    pub next_action_id: Item<'a, u64>,
    /// The action id being executed at the moment
    pub exec_action_id: Item<'a, u64>,
    /// The Croncat factory has all versions of other contracts,
    /// this should not change over time
    pub croncat_factory: Item<'a, CanonicalAddr>,
    /// Native denom for task topup
    pub native_denom: Item<'a, String>,
    /// Croncat state: Latest contract name to the version
    // perhaps can also move this to croncat_factory_sdk::state?
    pub latest_versions: Map<'a, &'a str, [u8; 2]>,
    /// Croncat state: last task executing on the manager
    pub last_task_execution_info: Item<'a, TaskExecutionInfo>,
    /// Croncat state: Latest task balance on the manager
    pub task_balances: Map<'a, &'a [u8], TaskBalance>,
}

#[contract]
impl CronKittyPlugin<'_> {
    pub const fn new() -> Self {
        Self {
            actions: Map::new("actions"),
            owner: Item::new("owner"),
            next_action_id: Item::new("id"),
            exec_action_id: Item::new("exec_id"),
            croncat_factory: Item::new("croncat-manager"),
            latest_versions: Map::new("latest_versions"),
            last_task_execution_info: Item::new("last_task_execution_info"),
            task_balances: Map::new("tasks_balances"),
            native_denom: Item::new("native_denom"),
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

        let contract_version =
            self.query_latest_version_croncat_contract(&deps.as_ref(), MANAGER)?;
        let manager_contract_addr =
            self.query_contract_addr(&deps.as_ref(), &contract_version, MANAGER)?;

        let config: CCConfig = deps
            .querier
            .query_wasm_smart(manager_contract_addr, &CCManagerQueryMsg::Config {})?;

        self.next_action_id.save(deps.storage, &0)?;
        self.native_denom.save(deps.storage, &config.native_denom)?;

        Ok(Response::new())
    }

    // This is called by the croncat manager contract to execute a task
    #[msg(exec)]
    pub fn execute(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        action_id: u64,
    ) -> Result<Response, ContractError> {
        let (deps, env, info) = ctx;
        execute::execute(self, deps, env, info, action_id)
    }

    #[msg(exec)]
    fn create_task(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        mut task: TaskRequest,
        mut auto_refill: Option<AutoRefill>,
    ) -> Result<Response, ContractError> {
        let (deps, env, info) = ctx;

        if info.funds.len() != 1 {
            return Err(ContractError::OneCoinExpectedForGas);
        }

        // only the owner (proxy) can create task
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else {
            // guarenteed by croncat that TASK and MANAGER are the same version
            let contract_version =
                self.query_latest_version_croncat_contract(&deps.as_ref(), TASK)?;
            let task_contract_addr =
                self.query_contract_addr(&deps.as_ref(), &contract_version, TASK)?;

            // If auto_refill is set, we make sure there is trigger_balance
            if let Some(mut r) = auto_refill.as_mut() {
                r.trigger_balance = r.trigger_balance.or_else(|| Some(info.funds[0].amount))
            }

            // The id for croncat to call back
            let id = self.next_action_id.load(deps.storage)?;
            self.actions.save(
                deps.storage,
                id,
                &ActionRef::new(
                    contract_version,
                    task.actions.iter().cloned().map(|a| a.msg).collect(),
                    auto_refill,
                ),
            )?;

            // This sums up all the action gas into one because croncat manager will only know the
            // action id on this contract to call.
            // Each task's gas_limit is provided by simulation in the frontend on croncat
            // TODO: We can do something similar to their contracts `validate_msg_calculate_usage` method
            // as well.
            //
            // Croncat logic:
            // Task contract calculates the total gas specified by user for each task,
            // it then creates fund balance for the task on the manager (who holds the funds sent)
            // On storing the task balance, the manager checks there is enough funds (gas_limit,
            // fees for croncat, native, cw20, ibc, etc)
            // The required fee per action is gas_base_fee + gas_action_fee + gas_limit +
            // treasury_fee + agent_fee. If it is not a one-off task, the fees are multipled by 2.
            //
            // Since Vectis Accounts will be self-custody, croncat only need to check that the gas is
            // enough. This is calculated in `execute_create_task_balance` on the manager
            // We are not checking it here

            let gas_limit = task.actions.iter().try_fold(0u64, |acc, a| {
                acc.checked_add(a.gas_limit.unwrap_or(0))
                    .ok_or(ContractError::Overflow)
            })?;

            let gas_limit = if gas_limit == 0 {
                None
            } else {
                Some(gas_limit)
            };

            // This is the action stored on Croncat contract
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
                    // TODO: This is the value the user provides for the task execution.
                    // https://github.com/CronCats/cw-croncat/issues/204
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

        // only the owner (proxy) can remove tasks
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else if let Some(action) = self.actions.may_load(deps.storage, task_id)? {
            let task = self.query_contract_addr(&deps.as_ref(), &action.version, TASK)?;
            let msg = SubMsg::reply_on_success(
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: task.to_string(),
                    msg: to_binary(&CCTaskExecMsg::RemoveTask {
                        task_hash: action.task_hash.ok_or(ContractError::TaskNotFound)?,
                    })?,
                    funds: vec![],
                }),
                task_id,
            );
            Ok(Response::new().add_submessage(msg))
        } else {
            Err(ContractError::TaskHashNotFound)
        }
    }

    #[msg(exec)]
    pub fn withdraw_funds(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
    ) -> Result<Response, ContractError> {
        let (deps, env, info) = ctx;
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else {
            let balances = deps
                .querier
                .query_all_balances(env.contract.address.as_str())?;

            if !balances.is_empty() {
                let msg = CosmosMsg::Bank(cosmwasm_std::BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: balances,
                });
                return Ok(Response::new().add_message(msg));
            }
            Ok(Response::new())
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

        // only the owner (proxy) can refill task
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized)
        } else {
            // call croncat to refill task
            if let Some(action) = self.actions.may_load(deps.storage, task_id)? {
                let manager = self.query_contract_addr(&deps.as_ref(), &action.version, MANAGER)?;
                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: manager.to_string(),
                    msg: to_binary(&CCManagerExecMsg::RefillTaskBalance {
                        task_hash: action.task_hash.ok_or(ContractError::TaskNotFound)?,
                    })?,
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
        let action = self.actions.load(deps.storage, action_id)?;
        let task_addr = self.query_contract_addr(&deps, &action.version, TASK)?;
        let manager_addr = self.query_contract_addr(&deps, &action.version, MANAGER)?;
        Ok(CronKittyActionResp {
            msgs: action.msgs,
            task_hash: action.task_hash,
            task_addr,
            manager_addr,
            auto_refill: action.refill_opt,
            refill_accounting: action.refill_accounting,
        })
    }

    #[msg(migrate)]
    fn migrate(&self, _ctx: (DepsMut, Env)) -> Result<Response, ContractError> {
        // Not used but required for impl for multitest
        Ok(Response::default())
    }

    pub(crate) fn query_latest_version_croncat_contract(
        &self,
        deps: &Deps,
        name: &str,
    ) -> Result<[u8; 2], ContractError> {
        let cc_factory = deps
            .api
            .addr_humanize(&self.croncat_factory.load(deps.storage)?)?;

        self.latest_versions
            .query(&deps.querier, cc_factory, name)
            .transpose()
            .ok_or_else(|| ContractError::NoCronCatVersion {
                name: name.to_string(),
            })?
            .map_err(|e| e.into())
    }

    /// Takes a CronCat contract name, queries the factory for the latest contract address.
    /// Returns a result with the latest version and the addr, or an error.
    pub(crate) fn query_contract_addr(
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
