use crate::tests::croncat_helpers::*;
pub use crate::{
    contract::{
        CronKittyPlugin, ExecMsg as CronKittyExecMsg, InstantiateMsg as CronKittyInstMsg,
        QueryMsg as CronKittyQueryMsg,
    },
    types::{AutoRefill, CronKittyActionResp},
};
use cosmwasm_std::{Addr, BankMsg, Empty};
use croncat_sdk_agents::msg::ExecuteMsg as AgentExecuteMsg;
pub use croncat_sdk_core::types::GasPrice;
use croncat_sdk_manager::{
    msg::{ManagerExecuteMsg, ManagerQueryMsg},
    types::TaskBalanceResponse,
};
use croncat_sdk_tasks::{
    msg::TasksQueryMsg,
    types::{Action, Interval, TaskInfo, TaskRequest},
};
use cw_multi_test::{App, Executor};
use vectis_contract_tests::common::common::*;
use vectis_contract_tests::common::{
    base_common::HubChainSuite,
    common::{proxy_exec, INSTALL_FEE, REGISTRY_FEE},
};
use vectis_wallet::{PluginParams, PluginPermissions, PluginSource, ProxyExecuteMsg};
pub const TASK_GAS_LIMIT: u64 = 150_000;

pub struct CronCatContracts {
    pub factory_addr: Addr,
    pub manager: Addr,
    pub tasks_addr: Addr,
}

pub fn setup_croncat_contracts(
    app: &mut App,
    deployer_signer: &Addr,
    controller: &Addr,
) -> CronCatContracts {
    app.send_tokens(
        controller.clone(),
        Addr::unchecked(ALICE),
        &[coin(100u128, DENOM)],
    )
    .unwrap();
    // ==============================================================
    // Instantiate Croncat and add Agent to execute tasks
    // ==============================================================
    let factory_addr = init_factory(app);

    let manager_instantiate_msg: croncat_sdk_manager::msg::ManagerInstantiateMsg =
        default_manager_instantiate_message();
    let manager = init_manager(
        app,
        &manager_instantiate_msg,
        &factory_addr,
        &[coin(1, DENOM)],
    );

    let agents_instantiate_msg: croncat_sdk_agents::msg::InstantiateMsg =
        default_agents_instantiate_message();
    let agents = init_agents(app, &agents_instantiate_msg, &factory_addr, &[]);

    let tasks_instantiate_msg: croncat_sdk_tasks::msg::TasksInstantiateMsg =
        default_tasks_instantiate_msg();
    let tasks_addr = init_tasks(app, &tasks_instantiate_msg, &factory_addr);

    // quick agent register
    // we pre allowed AGENT) into the whitelist on instantiation

    app.send_tokens(
        deployer_signer.clone(),
        Addr::unchecked(AGENT),
        &[coin(10_000_00, DENOM)],
    )
    .unwrap();

    let msg = AgentExecuteMsg::RegisterAgent {
        payable_account_id: Some(AGENT_BENEFICIARY.to_string()),
    };

    app.execute_contract(Addr::unchecked(AGENT), agents.clone(), &msg, &[])
        .unwrap();

    CronCatContracts {
        factory_addr,
        manager,
        tasks_addr,
    }
}

pub fn register_cronkitty(suite: &mut HubChainSuite, registry_fee: u128) {
    // ==============================================================
    // Upload Cronkitty and add to registry by the pluginCommittee
    // ==============================================================
    let cronkitty_code_id = suite.app.store_code(Box::new(CronKittyPlugin::new()));

    suite
        .app
        .execute_contract(
            suite.plugin_committee.clone(),
            suite.plugin_registry.clone(),
            &PRegistryExecMsg::RegisterPlugin {
                // this has to be same as crate name / contract_name
                name: "cronkitty".into(),
                creator: suite.deployer.to_string(),
                ipfs_hash: "some-hash".into(),
                version: VECTIS_VERSION.to_string(),
                code_id: cronkitty_code_id,
                checksum: "some-checksum".to_string(),
            },
            &[coin(registry_fee, DENOM)],
        )
        .unwrap();
}

pub fn set_up_proxy_and_install_cronkitty(
    suite: &mut HubChainSuite,
    install_fee: u128,
    plugin_id: u64,
    factory_addr: &Addr,
    initial_proxy_fund: u128,
) -> (Addr, Addr) {
    let mut funds = vec![];
    if initial_proxy_fund != 0 {
        funds.push(coin(initial_proxy_fund, DENOM))
    };
    let proxy = suite
        .create_new_proxy_without_guardians(
            suite.controller.clone(),
            funds,
            coin(WALLET_FEE, DENOM),
        )
        .unwrap();

    suite
        .app
        .execute_contract(
            suite.controller.clone(),
            proxy.clone(),
            &ProxyExecuteMsg::<Empty>::InstantiatePlugin {
                src: PluginSource::VectisRegistry(plugin_id),
                instantiate_msg: to_binary(&CronKittyInstMsg {
                    croncat_factory_addr: factory_addr.to_string(),
                    vectis_account_addr: proxy.to_string(),
                })
                .unwrap(),
                plugin_params: PluginParams {
                    permissions: vec![PluginPermissions::Exec],
                },
                label: "cronkitty-plugin".into(),
            },
            &[coin(install_fee, DENOM)],
        )
        .unwrap();

    (
        proxy.clone(),
        suite.query_installed_plugins(&proxy).unwrap().exec_plugins[0].clone(),
    )
}

pub fn create_task(
    suite: &mut HubChainSuite,
    proxy: &Addr,
    cronkitty: &Addr,
    gas_limit: u64,
    fund: Coin,
    call_back_msg: CosmosMsg,
    tasks_addr: &Addr,
    auto_refill: Option<AutoRefill>,
) -> Vec<TaskInfo> {
    let task = TaskRequest {
        interval: Interval::Block(5),
        boundary: None,
        stop_on_fail: false,
        actions: vec![Action {
            msg: call_back_msg,
            gas_limit: Some(gas_limit),
        }],
        queries: None,
        transforms: None,
        cw20: None,
    };

    suite
        .app
        .execute_contract(
            suite.controller.clone(),
            proxy.clone(),
            &proxy_exec(
                &cronkitty,
                &CronKittyExecMsg::CreateTask { task, auto_refill },
                vec![fund],
            ),
            &[],
        )
        .unwrap();

    suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: tasks_addr.to_string(),
            msg: to_binary(&TasksQueryMsg::TasksByOwner {
                owner_addr: cronkitty.to_string(),
                from_index: None,
                limit: None,
            })
            .unwrap(),
        }))
        .unwrap()
}

pub fn query_task_balance(
    suite: &mut HubChainSuite,
    mgmt_addr: &Addr,
    task_hash: String,
) -> TaskBalanceResponse {
    suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: mgmt_addr.to_string(),
            msg: to_binary(&ManagerQueryMsg::TaskBalance { task_hash }).unwrap(),
        }))
        .unwrap()
}

pub fn get_require_fund(gas_limit: u64) -> u128 {
    let gas_price_on_mgr = GasPrice::default();
    // The first *2 is due to the fact that croncat makes it at least 2 for re-occuring tasks
    // the second *2 is a buffer for watermark  calculation
    let required = gas_price_on_mgr
        .calculate(gas_limit + AGENT_FEE + TREASURY_FEE + GAS_BASE_FEE + GAS_ACTION_FEE)
        .unwrap()
        * 2
        * 2;
    required
}

pub fn agent_proxy_call(suite: &mut HubChainSuite, manager: &Addr) -> AppResponse {
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    suite
        .app
        .execute_contract(
            Addr::unchecked(AGENT),
            manager.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap()
}

pub fn query_action_response(
    suite: &HubChainSuite,
    cronkitty: &Addr,
    action_id: u64,
) -> CronKittyActionResp {
    let res: CronKittyActionResp = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cronkitty.to_string(),
            msg: to_binary(&CronKittyQueryMsg::Action { action_id }).unwrap(),
        }))
        .unwrap();
    res
}

pub fn filter_event(res: &AppResponse, event_type: &str) -> Vec<Event> {
    res.events
        .iter()
        .cloned()
        .filter(|event| event.ty == event_type)
        .collect()
}

pub fn mock_setup_a_task(
    suite: &mut HubChainSuite,
    cc_contracts: &CronCatContracts,
    auto_refill: Option<AutoRefill>,
    msg: Option<CosmosMsg>,
) -> (TaskInfo, Addr, Addr) {
    register_cronkitty(suite, REGISTRY_FEE);
    let (proxy, cronkitty) = set_up_proxy_and_install_cronkitty(
        suite,
        INSTALL_FEE,
        1,
        &cc_contracts.factory_addr,
        100_000_000,
    );
    let required = get_require_fund(TASK_GAS_LIMIT);
    let msg = msg.unwrap_or(CosmosMsg::Bank(BankMsg::Burn {
        amount: vec![coin(1, DENOM)],
    }));

    let tasks_on_croncat = create_task(
        suite,
        &proxy,
        &cronkitty,
        TASK_GAS_LIMIT,
        coin(required, DENOM),
        msg.clone(),
        &cc_contracts.tasks_addr,
        auto_refill,
    );
    (tasks_on_croncat[0].clone(), proxy, cronkitty)
}
