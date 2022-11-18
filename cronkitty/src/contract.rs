use crate::error::ContractError;
use cosmwasm_std::{
    to_binary, CanonicalAddr, CosmosMsg, DepsMut, Env, MessageInfo, Response, SubMsg, WasmMsg,
};
use cw2::set_contract_version;
use cw_croncat_core::{msg::TaskRequest, types::Action};
use cw_storage_plus::{Item, Map};
use sylvia::contract;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct CronKittyPlugin<'a> {
    /// action_id: task_hash, actions
    pub(crate) actions: Map<'a, u64, Vec<Action>>,
    pub(crate) owner: Item<'a, CanonicalAddr>,
    pub(crate) action_id: Item<'a, u64>,
}

#[contract]
impl CronKittyPlugin<'_> {
    pub const fn new() -> Self {
        Self {
            actions: Map::new("actions"),
            owner: Item::new("owner"),
            action_id: Item::new("id"),
        }
    }

    #[msg(instantiate)]
    pub fn instantiate(&self, ctx: (DepsMut, Env, MessageInfo)) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        self.owner.save(
            deps.storage,
            &deps.api.addr_canonicalize(&info.sender.as_str())?,
        )?;
        Ok(Response::new())
    }
    #[msg(exec)]
    fn execute(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        action_id: u64,
    ) -> Result<Response, ContractError> {
        Ok(Response::new())
    }

    #[msg(exec)]
    fn add_task(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        mut tq: TaskRequest,
    ) -> Result<Response, ContractError> {
        let (deps, env, info) = ctx;
        if info.sender != deps.api.addr_humanize(&self.owner.load(deps.storage)?)? {
            Err(ContractError::Unauthorized {})
        } else {
            let id = self.action_id.load(deps.storage)?;

            self.actions.save(deps.storage, id, &tq.actions.clone())?;
            let action = Action {
                msg: CosmosMsg::<()>::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecMsg::Execute { action_id: id })?,
                    funds: vec![],
                }),
                // what is right here?
                gas_limit: None,
            };

            //Pub struct TaskRequest {
            //    pub interval: Interval,
            //    pub boundary: Option<Boundary>,
            //    pub stop_on_fail: bool,
            //    pub actions: Vec<Action>,
            //    pub rules: Option<Vec<Rule>>,
            //    pub cw20_coins: Vec<Cw20Coin>,
            //}
            //

            self.action_id.update(deps.storage, |id| {
                id.checked_add(1).ok_or(ContractError::Overflow)
            })?;
            Ok(Response::new())
        }
    }
}
