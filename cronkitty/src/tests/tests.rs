pub use crate::contract::{
    CronKittyActionResp, CronKittyPlugin, ExecMsg as CronKittyExecMsg,
    InstantiateMsg as CronKittyInstMsg, QueryMsg as CronKittyQueryMsg,
};
use crate::tests::helpers::*;
use cosmwasm_std::{
    coin, to_binary, Addr, BankMsg, CosmosMsg, Empty, QueryRequest, StdError, Uint128, WasmQuery,
};
use croncat_sdk_agents::msg::ExecuteMsg as AgentExecuteMsg;
use croncat_sdk_manager::{
    msg::{ManagerExecuteMsg, ManagerQueryMsg},
    types::TaskBalanceResponse,
};
use croncat_sdk_tasks::{
    msg::{TasksInstantiateMsg, TasksQueryMsg},
    types::{Action, Interval, TaskInfo, TaskRequest, TaskResponse},
};
use cw_multi_test::Executor;
use vectis_contract_tests::common::common::proxy_exec;
use vectis_contract_tests::common::plugins_common::PluginsSuite;
use vectis_wallet::{PluginParams, ProxyExecuteMsg};

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
    let mut suite = PluginsSuite::init().unwrap();

    // ==============================================================
    // Instantiate Croncat and add Agent to execute tasks
    // ==============================================================
    let factory_addr = init_factory(&mut suite.ds.app);
    let instantiate_msg: TasksInstantiateMsg = default_instantiate_msg();
    let tasks_addr = init_tasks(&mut suite.ds.app, &instantiate_msg, &factory_addr);
    let manager_addr = init_manager(&mut suite.ds.app, &factory_addr);
    let agents_addr = init_agents(&mut suite.ds.app, &factory_addr);

    // quick agent register
    suite
        .ds
        .app
        .send_tokens(
            suite.ds.deployer.clone(),
            Addr::unchecked(AGENT0),
            &[coin(10_000_00, DENOM)],
        )
        .unwrap();

    let msg = AgentExecuteMsg::RegisterAgent {
        payable_account_id: Some(AGENT_BENEFICIARY.to_string()),
    };
    suite
        .ds
        .app
        .execute_contract(Addr::unchecked(AGENT0), agents_addr.clone(), &msg, &[])
        .unwrap();

    // This fast forwards 10 blocks and arg is timestamp
    suite.ds.fast_forward_block_time(10000);

    // ==============================================================
    // Instantiate CronKitty
    // ==============================================================
    let cronkitty_code_id = suite.ds.app.store_code(Box::new(CronKittyPlugin::new()));
    suite
        .ds
        .app
        .execute_contract(
            suite.ds.controller.clone(),
            suite.proxy.clone(),
            &ProxyExecuteMsg::<Empty>::InstantiatePlugin {
              code_id: 0,
              instantiate_msg: to_binary(&CronKittyInstMsg {
                    croncat_factory_addr: factory_addr.to_string(),
                  }).unwrap(),
              plugin_params: PluginParams { grantor: false },
                label: "cronkitty-plugin".into(),
            },
            &[coin(10000, DENOM)],
        )
        .unwrap();

    let cronkitty = suite.query_installed_plugins(None, None).unwrap().plugins[0].clone();

    // ==============================================================
    // Create Task on Cronkitty + Croncat
    // ==============================================================

    let to_send_amount = 500;
    suite
        .ds
        .app
        .send_tokens(
            suite.ds.deployer.clone(),
            suite.proxy.clone(),
            &[coin(to_send_amount, DENOM)],
        )
        .unwrap();

    let init_proxy_balance = suite.ds.query_balance(&suite.proxy).unwrap();

    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: suite.ds.dao.to_string(),
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
        .ds
        .app
        .execute_contract(
            suite.ds.controller.clone(),
            suite.proxy.clone(),
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
        .ds
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
        .ds
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
    suite.ds.fast_forward_block_time(10000);
    // ==============================================================
    // Agent executes proxy call
    // ==============================================================

    // There is only one task in the queue
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    // We ask agent to execute the one task
    suite
        .ds
        .app
        .execute_contract(
            Addr::unchecked(AGENT0),
            manager_addr.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap();

    let after_proxy_balance = suite.ds.query_balance(&suite.proxy).unwrap();

    // Ensure it happened
    assert_eq!(
        init_proxy_balance.amount - after_proxy_balance.amount,
        Uint128::from(to_send_amount)
    );
    // ==============================================================
    // Proxy refill task from cronkitty and croncat
    // ==============================================================

    let before_refill_tasks: TaskBalanceResponse = suite
        .ds
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: manager_addr.to_string(),
            msg: to_binary(&ManagerQueryMsg::TaskBalance {
                task_hash: task_hash.clone(),
            })
            .unwrap(),
        }))
        .unwrap();

    let refill_amount = 150_000;
    suite
        .ds
        .app
        .execute_contract(
            suite.ds.controller.clone(),
            suite.proxy.clone(),
            &proxy_exec(
                &cronkitty,
                &CronKittyExecMsg::RefillTask { task_id: 0 },
                vec![coin(refill_amount, DENOM)],
            ),
            // to send exact amount needed to the proxy so balance doesnt change
            &[coin(refill_amount, DENOM)],
        )
        .unwrap();
    suite.ds.fast_forward_block_time(10000);

    let after_refill_tasks: TaskBalanceResponse = suite
        .ds
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: manager_addr.to_string(),
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
        .ds
        .app
        .execute_contract(
            suite.ds.controller.clone(),
            suite.proxy.clone(),
            &proxy_exec(
                &cronkitty,
                &CronKittyExecMsg::RemoveTask { task_id: 0 },
                vec![],
            ),
            // to send exact amount needed to the proxy so balance doesnt change
            &[],
        )
        .unwrap();
    suite.ds.fast_forward_block_time(10000);

    // Removed on croncat
    let after_remove_task: Vec<TaskResponse> = suite
        .ds
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
            .ds
            .app
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: cronkitty.to_string(),
                msg: to_binary(&CronKittyQueryMsg::Action { action_id: 0 }).unwrap(),
            }));

    result.unwrap_err();
}
