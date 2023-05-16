pub use crate::contract::{
    CronKittyActionResp, CronKittyPlugin, ExecMsg as CronKittyExecMsg,
    InstantiateMsg as CronKittyInstMsg, QueryMsg as CronKittyQueryMsg,
};
use crate::tests::helpers::*;
use cosmwasm_std::{
    coin, to_binary, Addr, BankMsg, CosmosMsg, Empty, QueryRequest, StdError, Uint128, WasmQuery,
};
use croncat_integration_testing::{
    test_helpers::{
        add_seconds_to_block, default_agents_instantiate_message,
        default_manager_instantiate_message, default_tasks_instantiate_msg, increment_block_height,
        init_agents, init_factory, init_manager, init_tasks,
    },
    AGENT,
};
use croncat_sdk_agents::msg::ExecuteMsg as AgentExecuteMsg;
use croncat_sdk_manager::{
    msg::{ManagerExecuteMsg, ManagerQueryMsg},
    types::TaskBalanceResponse,
};
use croncat_sdk_tasks::{
    msg::TasksQueryMsg,
    types::{Action, Interval, TaskInfo, TaskRequest, TaskResponse},
};
use cw_multi_test::Executor;
use vectis_contract_tests::common::{
    base_common::HubChainSuite,
    common::{proxy_exec, INSTALL_FEE, REGISTRY_FEE},
    plugins::*,
};
use vectis_wallet::{
    PluginParams, PluginPermissions, PluginSource, ProxyExecuteMsg, RegisterPlugin,
};

// TODO: add registry as cronkitty is trusted
//
//  This is a full cycle integration test with
//  - croncat contracts (factory, agent, manager, tasks)
//  - proxy
//  - cronkitty contract
//
//  Test is what a user might go through
//  - create task (once? )
//  - agent executes task via croncat -> cronkitty -> proxy
//  - refill task
//  - remote task (get refund)

#[test]
fn cronkitty_plugin_works() {
    let mut suite = HubChainSuite::init().unwrap();

    // ==============================================================
    // Instantiate Croncat and add Agent to execute tasks
    // ==============================================================
    let factory = init_factory(&mut suite.app);

    let manager_instantiate_msg: croncat_sdk_manager::msg::ManagerInstantiateMsg =
        default_manager_instantiate_message();
    let manager = init_manager(
        &mut suite.app,
        &manager_instantiate_msg,
        &factory,
        &[coin(1, DENOM)],
    );

    let agents_instantiate_msg: croncat_sdk_agents::msg::InstantiateMsg =
        default_agents_instantiate_message();
    let agents = init_agents(&mut suite.app, &agents_instantiate_msg, &factory_addr, &[]);

    let tasks_instantiate_msg: croncat_sdk_tasks::msg::TasksInstantiateMsg =
        default_tasks_instantiate_msg();
    let tasks_addr = init_tasks(&mut suite.app, &tasks_instantiate_msg, &factory_addr);

    // quick agent register
    // we pre allowed AGENT) into the whitelist on instantiation
    suite
        .app
        .send_tokens(
            suite.deployer_signer.clone(),
            Addr::unchecked(AGENT),
            &[coin(10_000_00, DENOM)],
        )
        .unwrap();

    let msg = AgentExecuteMsg::RegisterAgent {
        payable_account_id: Some(AGENT_BENEFICIARY.to_string()),
    };
    suite
        .app
        .execute_contract(Addr::unchecked(AGENT), agents.clone(), &msg, &[])
        .unwrap();

    // This fast forwards 10 blocks and arg is timestamp
    suite
        .app
        .update_block(|block| add_seconds_to_block(block, 10));
    suite
        .app
        .update_block(|block| increment_block_height(block, Some(2)));

    // ==============================================================
    // Upload Cronkitty and add to registry by the pluginCommittee
    // ==============================================================
    let cronkitty_code_id = suite.app.store_code(Box::new(CronKittyPlugin::new()));

    suite
        .app
        .execute_contract(
            suite.plugin_committee.clone(),
            suite.plugin_registry.clone(),
            &RegisterPlugin {
                name: "Cronkitty".into(),
                creator: suite.deployer.to_string(),
                ipfs_hash: "some-hash".into(),
                version: "1.0".to_string(),
                code_id: cronkitty_code_id,
                checksum: "some-checksum".to_string(),
            },
            &[coin(REGISTRY_FEE, DENOM)],
        )
        .unwrap();

    let plugines = suite.query_registered_plugins(None, None).unwrap();
    let plugin_id = plugines.total;

    // ==============================================================
    // Vectis Account controller installs plugin via Vectis Plugin Registry
    // ==============================================================
    let proxy = suite
        .create_new_proxy_without_guardians(
            suite.controller.clone(),
            vec![],
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
                    croncat_factory_addr: factory.to_string(),
                    vectis_account_addr: proxy.to_string(),
                })
                .unwrap(),
                plugin_params: PluginParams {
                    permissions: vec![PluginPermissions::Exec],
                },
                label: "cronkitty-plugin".into(),
            },
            &[coin(INSTALL_FEE + 0u128, DENOM)],
        )
        .unwrap();

    let cronkitty = suite.query_installed_plugins(&proxy).unwrap().exec_plugins[0].clone();

    // ==============================================================
    // Create Task on Cronkitty + Croncat
    // ==============================================================

    let to_send_amount = 100;
    suite
        .app
        .send_tokens(
            suite.controller.clone(),
            proxy.clone(),
            &[coin(to_send_amount, DENOM)],
        )
        .unwrap();

    let init_proxy_balance = suite.query_balance(&proxy).unwrap();

    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: suite.deployer.to_string(),
        amount: vec![coin(to_send_amount, DENOM)],
    });

    let task = TaskRequest {
        interval: Interval::Block(5),
        boundary: None,
        stop_on_fail: false,
        actions: vec![Action {
            msg: msg.clone(),
            gas_limit: Some(150_000),
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
                vec![coin(150_000, DENOM)],
            ),
            // to send exact amount needed to the proxy so balance doesnt change
            &[coin(150_000, DENOM)],
        )
        .unwrap();

    // Check task is added on Croncat Tasks
    let tasks: Vec<TaskInfo> = suite
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
        .unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].clone().owner_addr, cronkitty);
    let task_hash = tasks[0].clone().task_hash;

    // Checks that the task is stored in Actions on CronKitty
    let action: CronKittyActionResp = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cronkitty.to_string(),
            msg: to_binary(&CronKittyQueryMsg::Action { action_id: 0 }).unwrap(),
        }))
        .unwrap();

    assert_eq!(action.msgs[0], msg);
    assert_eq!(action.task_hash.unwrap(), task_hash);

    // This fast forwards 10 blocks and arg is timestamp
    suite.fast_forward_block_time(10000);
    // ==============================================================
    // Agent executes proxy call
    // ==============================================================

    // There is only one task in the queue
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    // We ask agent to execute the one task
    suite
        .app
        .execute_contract(
            Addr::unchecked(AGENT),
            manager.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap();

    let after_proxy_balance = suite.query_balance(&proxy).unwrap();

    // Ensure it happened
    assert_eq!(
        init_proxy_balance.amount - after_proxy_balance.amount,
        Uint128::from(to_send_amount)
    );
    // ==============================================================
    // Proxy refill task from cronkitty and croncat
    // ==============================================================

    let before_refill_tasks: TaskBalanceResponse = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: manager.to_string(),
            msg: to_binary(&ManagerQueryMsg::TaskBalance {
                task_hash: task_hash.clone(),
            })
            .unwrap(),
        }))
        .unwrap();

    let refill_amount = 150_000;
    suite
        .app
        .execute_contract(
            suite.controller.clone(),
            proxy.clone(),
            &proxy_exec(
                &cronkitty,
                &CronKittyExecMsg::RefillTask { task_id: 0 },
                vec![coin(refill_amount, DENOM)],
            ),
            // to send exact amount needed to the proxy so balance doesnt change
            &[coin(refill_amount, DENOM)],
        )
        .unwrap();
    suite.fast_forward_block_time(10000);

    let after_refill_tasks: TaskBalanceResponse = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: manager.to_string(),
            msg: to_binary(&ManagerQueryMsg::TaskBalance { task_hash }).unwrap(),
        }))
        .unwrap();

    assert_eq!(
        after_refill_tasks.balance.unwrap().native_balance
            - before_refill_tasks.balance.unwrap().native_balance,
        Uint128::from(refill_amount)
    );

    // ==============================================================
    // Proxy remove task from cronkitty and croncat
    // ==============================================================
    suite
        .app
        .execute_contract(
            suite.controller.clone(),
            proxy.clone(),
            &proxy_exec(
                &cronkitty,
                &CronKittyExecMsg::RemoveTask { task_id: 0 },
                vec![],
            ),
            // to send exact amount needed to the proxy so balance doesnt change
            &[],
        )
        .unwrap();
    suite.fast_forward_block_time(10000);

    // Removed on croncat
    let after_remove_task: Vec<TaskResponse> = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: tasks_addr.to_string(),
            msg: to_binary(&TasksQueryMsg::Tasks {
                from_index: None,
                limit: None,
            })
            .unwrap(),
        }))
        .unwrap();

    assert!(after_remove_task.is_empty());

    // Checks that it is removed on cronkitty
    let result: Result<CronKittyActionResp, StdError> =
        suite
            .app
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: cronkitty.to_string(),
                msg: to_binary(&CronKittyQueryMsg::Action { action_id: 0 }).unwrap(),
            }));

    result.unwrap_err();
}
