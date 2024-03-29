pub use crate::contract::{
    CronKittyActionResp, CronKittyPlugin, ExecMsg as CronKittyExecMsg,
    InstantiateMsg as CronKittyInstMsg, QueryMsg as CronKittyQueryMsg,
};
use crate::tests::croncat_helpers::*;
use cosmwasm_std::{Addr, BankMsg, Empty};
use croncat_sdk_agents::msg::ExecuteMsg as AgentExecuteMsg;
pub use croncat_sdk_core::types::GasPrice;
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
                &CronKittyExecMsg::CreateTask { task },
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

pub fn mock_setup_a_task(
    suite: &mut HubChainSuite,
    cc_contracts: &CronCatContracts,
) -> (TaskInfo, Addr, Addr) {
    register_cronkitty(suite, REGISTRY_FEE);
    let (proxy, cronkitty) = set_up_proxy_and_install_cronkitty(
        suite,
        INSTALL_FEE,
        1,
        &cc_contracts.factory_addr,
        100_000,
    );
    let gas_limit = 150_000u64;
    let gas_price_on_mgr = GasPrice::default();
    let required = gas_price_on_mgr
        .calculate(gas_limit + AGENT_FEE + TREASURY_FEE + GAS_BASE_FEE + GAS_ACTION_FEE)
        .unwrap()
        * 2;
    let msg = CosmosMsg::Bank(BankMsg::Burn {
        amount: vec![coin(1, DENOM)],
    });

    let tasks_on_croncat = create_task(
        suite,
        &proxy,
        &cronkitty,
        gas_limit,
        coin(required, DENOM),
        msg.clone(),
        &cc_contracts.tasks_addr,
    );
    (tasks_on_croncat[0].clone(), proxy, cronkitty)
}
