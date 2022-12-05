use crate::error::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, CanonicalAddr, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdResult, SubMsg, WasmMsg,
};
use cw2::set_contract_version;
use cw_croncat_core::{
    msg::{ExecuteMsg as CCExecMsg, TaskRequest},
    types::Action,
};
use cw_storage_plus::{Item, Map};
use sylvia::contract;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
pub struct StoredMsgsResp {
    msgs: Vec<CosmosMsg>,
}

pub struct CronKittyPlugin<'a> {
    pub actions: Map<'a, u64, Vec<CosmosMsg>>,
    pub owner: Item<'a, CanonicalAddr>,
    pub action_id: Item<'a, u64>,
    pub croncat: Item<'a, CanonicalAddr>,
}

#[contract]
impl CronKittyPlugin<'_> {
    pub const fn new() -> Self {
        Self {
            actions: Map::new("actions"),
            owner: Item::new("owner"),
            action_id: Item::new("id"),
            croncat: Item::new("croncat"),
        }
    }

    #[msg(instantiate)]
    pub fn instantiate(
        &self,
        ctx: (DepsMut, Env, MessageInfo),
        croncat_addr: String,
    ) -> Result<Response, ContractError> {
        let (deps, _, info) = ctx;
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        self.owner.save(
            deps.storage,
            &deps.api.addr_canonicalize(&info.sender.as_str())?,
        )?;
        let croncat = deps
            .api
            .addr_canonicalize(&deps.api.addr_validate(&croncat_addr)?.as_str())?;
        self.croncat.save(deps.storage, &croncat)?;
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
        if info.sender != deps.api.addr_humanize(&self.croncat.load(deps.storage)?)? {
            Err(ContractError::Unauthorized {})
        } else {
            let actions = self.actions.load(deps.storage, action_id)?;
            // These msgs should call the owner proxy contract
            // Proxy contract will give permission to this plugin to call itself
            Ok(Response::new().add_messages(actions))
        }
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
            self.actions.save(
                deps.storage,
                id,
                &tq.actions.iter().cloned().map(|a| a.msg).collect(),
            )?;

            let action = Action {
                msg: CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecMsg::Execute { action_id: id })?,
                    funds: vec![],
                }),
                // what is right here?
                gas_limit: Some(150_000),
            };

            // We forward all the other params (so we can contribute / use to frontend code)
            // The Action called is to call this plugin at the given intervals
            tq.actions = vec![action];
            tq.cw20_coins = vec![];

            let msg = SubMsg::reply_always(
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: deps
                        .api
                        .addr_humanize(&self.croncat.load(deps.storage)?)?
                        .to_string(),
                    msg: to_binary(&CCExecMsg::CreateTask { task: tq })?,
                    // TODO: find out how much to send here
                    funds: info.funds,
                }),
                id,
            );

            Ok(Response::new().add_submessage(msg))
        }
    }

    #[msg(query)]
    pub fn action_id(&self, ctx: (Deps, Env)) -> StdResult<u64> {
        let (deps, _) = ctx;
        self.action_id.load(deps.storage)
    }

    #[msg(query)]
    pub fn action(&self, ctx: (Deps, Env), action_id: u64) -> StdResult<StoredMsgsResp> {
        let (deps, _) = ctx;
        let msgs = self.actions.load(deps.storage, action_id)?;
        Ok(StoredMsgsResp { msgs })
    }
}
