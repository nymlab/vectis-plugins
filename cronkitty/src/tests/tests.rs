pub use crate::contract::{
    CronKittyActionResp, CronKittyPlugin, ExecMsg as CronKittyExecMsg,
    InstantiateMsg as CronKittyInstMsg, QueryMsg as CronKittyQueryMsg,
};
use crate::tests::{croncat_helpers::*, helpers::*};
use cosmwasm_std::{
    coin, to_binary, Addr, BankMsg, CosmosMsg, QueryRequest, StdError, Uint128, WasmQuery,
};
use croncat_sdk_manager::{
    msg::{ManagerExecuteMsg, ManagerQueryMsg},
    types::TaskBalanceResponse,
};
use croncat_sdk_tasks::{
    msg::{TasksExecuteMsg, TasksQueryMsg},
    types::{Action, Interval, TaskInfo, TaskRequest, TaskResponse},
};
use cw_multi_test::Executor;
use vectis_contract_tests::common::{
    base_common::HubChainSuite,
    common::{proxy_exec, INSTALL_FEE, REGISTRY_FEE},
    plugins::*,
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
fn install_cronkitty_works() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);

    // This fast forwards 10 blocks and arg is timestamp
    suite.fast_forward_block_time(10);

    register_cronkitty(&mut suite, REGISTRY_FEE);
    let plugins = suite.query_registered_plugins(None, None).unwrap();
    let plugin_id = plugins.total;

    let initial_proxy_fund = 100_000;
    let (proxy, cronkitty) = set_up_proxy_and_install_cronkitty(
        &mut suite,
        INSTALL_FEE,
        plugin_id,
        &cc_contracts.factory_addr,
        initial_proxy_fund,
    );

    let registered_plugins = suite.query_registered_plugins(None, None).unwrap();
    assert_eq!(registered_plugins.current_plugin_id, 1);
    assert_eq!(registered_plugins.total, 1);

    let plugins = suite.query_installed_plugins(&proxy).unwrap();
    assert!(plugins.exec_plugins.contains(&cronkitty));
}

#[test]
fn correct_gas_fee_for_task_creation_works() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);

    register_cronkitty(&mut suite, REGISTRY_FEE);
    let plugin_id = 1;

    let initial_proxy_fund = 100_000;
    let (proxy, cronkitty) = set_up_proxy_and_install_cronkitty(
        &mut suite,
        INSTALL_FEE,
        plugin_id,
        &cc_contracts.factory_addr,
        initial_proxy_fund,
    );

    let plugins = suite.query_installed_plugins(&proxy).unwrap();
    assert!(plugins.exec_plugins.contains(&cronkitty));

    // ==============================================================
    // Create Task on Cronkitty + Croncat
    // ==============================================================
    let to_send_amount = 5u128;
    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: suite.deployer.to_string(),
        amount: vec![coin(to_send_amount, DENOM)],
    });
    let gas_limit = 150_000u64;
    let gas_price_on_mgr = GasPrice::default();
    let required = gas_price_on_mgr
        .calculate(gas_limit + AGENT_FEE + TREASURY_FEE + GAS_BASE_FEE + GAS_ACTION_FEE)
        .unwrap()
        * 2;
    let tasks_on_croncat = create_task(
        &mut suite,
        &proxy,
        &cronkitty,
        gas_limit,
        coin(required, DENOM),
        msg.clone(),
        &cc_contracts.tasks_addr,
        None,
    );

    assert_eq!(tasks_on_croncat.len(), 1);
    assert_eq!(tasks_on_croncat[0].clone().owner_addr, cronkitty);
    let task_hash = tasks_on_croncat[0].clone().task_hash;

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
}

#[test]
fn refill_works() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);
    let (task_on_croncat, proxy, cronkitty) = mock_setup_a_task(&mut suite, &cc_contracts, None);

    let before_refill_tasks: TaskBalanceResponse = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cc_contracts.manager.to_string(),
            msg: to_binary(&ManagerQueryMsg::TaskBalance {
                task_hash: task_on_croncat.task_hash.clone(),
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
            contract_addr: cc_contracts.manager.to_string(),
            msg: to_binary(&ManagerQueryMsg::TaskBalance {
                task_hash: task_on_croncat.task_hash.clone(),
            })
            .unwrap(),
        }))
        .unwrap();

    assert_eq!(
        after_refill_tasks.balance.unwrap().native_balance
            - before_refill_tasks.balance.unwrap().native_balance,
        Uint128::from(refill_amount)
    );
}

#[test]
fn remove_task_works() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);
    let (task_on_croncat, proxy, cronkitty) = mock_setup_a_task(&mut suite, &cc_contracts, None);

    // check it was added to cronkitty
    let _: CronKittyActionResp = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cronkitty.to_string(),
            msg: to_binary(&CronKittyQueryMsg::Action { action_id: 0 }).unwrap(),
        }))
        .unwrap();

    // the balance of proxy now
    let init_proxy_balance = suite.query_balance(&proxy).unwrap();
    let init_cronkitty_balance = suite.query_balance(&cronkitty).unwrap();
    let task_balance_on_croncat =
        query_task_balance(&mut suite, &cc_contracts.manager, task_on_croncat.task_hash);

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
            &[],
        )
        .unwrap();
    suite.fast_forward_block_time(10000);

    // Removed on croncat
    let after_remove_task: Vec<TaskResponse> = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cc_contracts.tasks_addr.to_string(),
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

    let after_proxy_balance = suite.query_balance(&proxy).unwrap();
    let after_cronkitty_balance = suite.query_balance(&cronkitty).unwrap();

    assert_eq!(
        after_proxy_balance
            .amount
            .checked_sub(init_proxy_balance.amount)
            .unwrap(),
        task_balance_on_croncat.balance.unwrap().native_balance
    );
    assert_eq!(after_cronkitty_balance, init_cronkitty_balance)
}

#[test]
fn remove_task_cannot_be_done_by_others() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);
    let (_task_on_croncat, _proxy, cronkitty) = mock_setup_a_task(&mut suite, &cc_contracts, None);
    suite
        .app
        .execute_contract(
            suite.deployer.clone(),
            cronkitty.clone(),
            &CronKittyExecMsg::RemoveTask { task_id: 0 },
            &[],
        )
        .unwrap_err();
}

#[test]
#[should_panic]
fn insufficient_fee_cannot_create_task() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);

    register_cronkitty(&mut suite, REGISTRY_FEE);
    let plugin_id = 1;
    let initial_proxy_fund = 100_000;
    let (proxy, cronkitty) = set_up_proxy_and_install_cronkitty(
        &mut suite,
        INSTALL_FEE,
        plugin_id,
        &cc_contracts.factory_addr,
        initial_proxy_fund,
    );

    let msg = CosmosMsg::Bank(BankMsg::Burn {
        amount: vec![coin(100, DENOM)],
    });
    let gas_limit = 150_000u64;
    let gas_price_on_mgr = GasPrice::default();
    let required = gas_price_on_mgr
        .calculate(gas_limit + AGENT_FEE + TREASURY_FEE + GAS_BASE_FEE + GAS_ACTION_FEE)
        .unwrap()
        * 2;
    create_task(
        &mut suite,
        &proxy,
        &cronkitty,
        gas_limit,
        coin(required / 4, DENOM),
        msg.clone(),
        &cc_contracts.tasks_addr,
        None,
    );
}

#[test]
fn cronkitty_actions_cannot_execute_by_other_tasks() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);

    // make sure we already have a task on cronkitty to be called by others
    let (_task_on_croncat, _proxy, cronkitty) = mock_setup_a_task(&mut suite, &cc_contracts, None);

    // now a malicious account creates to call our task
    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cronkitty.to_string(),
        msg: to_binary(&CronKittyExecMsg::Execute { action_id: 0 }).unwrap(),
        funds: vec![],
    });

    let gas_limit = 150_000u64;
    let gas_price_on_mgr = GasPrice::default();
    let required = gas_price_on_mgr
        .calculate(gas_limit + AGENT_FEE + TREASURY_FEE + GAS_BASE_FEE + GAS_ACTION_FEE)
        .unwrap()
        * 2;
    let task = TaskRequest {
        interval: Interval::Immediate,
        boundary: None,
        stop_on_fail: false,
        actions: vec![Action {
            msg,
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
            cc_contracts.tasks_addr.clone(),
            &TasksExecuteMsg::CreateTask {
                task: Box::new(task),
            },
            &[coin(required, DENOM)],
        )
        .unwrap();

    // This fast forwards 10 blocks and arg is timestamp
    suite.fast_forward_block_time(10000);

    // ==============================================================
    // Agent executes malicious msg
    // ==============================================================
    let malicious_tasks: Vec<TaskInfo> = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cc_contracts.tasks_addr.to_string(),
            msg: to_binary(&TasksQueryMsg::TasksByOwner {
                owner_addr: suite.controller.to_string(),
                from_index: None,
                limit: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(malicious_tasks.len(), 1);

    let current_task: TaskResponse = suite
        .app
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cc_contracts.tasks_addr.to_string(),
            msg: to_binary(&TasksQueryMsg::CurrentTask {}).unwrap(),
        }))
        .unwrap();

    assert_eq!(
        current_task.task.unwrap().task_hash,
        malicious_tasks[0].task_hash.clone()
    );

    // None - This means not an evented task, but we already checked the task it will execute
    // `current_task` is going to be the malicious task
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    let res = suite
        .app
        .execute_contract(
            Addr::unchecked(AGENT),
            cc_contracts.manager.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap();

    // We find the expected error
    let wasm_events = res.events.iter().filter(|event| {
        event.ty == "wasm"
            && event
                .attributes
                .iter()
                .find(|attr| attr.key == "action0_failure")
                .is_some()
            || (event.ty == "wasm"
                && event
                    .attributes
                    .iter()
                    .find(|attr| attr.value == malicious_tasks[0].task_hash.clone())
                    .is_some())
    });
    assert_eq!(wasm_events.count(), 2);
}

#[test]
fn legit_task_on_cronkitty_can_be_executed() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);

    // make sure we already have a task on cronkitty to be called by others
    let (_, _proxy, _) = mock_setup_a_task(&mut suite, &cc_contracts, None);
    suite.fast_forward_block_time(10000);
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    let res = suite
        .app
        .execute_contract(
            Addr::unchecked(AGENT),
            cc_contracts.manager.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap();

    let wasm_events = res
        .events
        .iter()
        .filter(|event| event.ty == "wasm-vectis.proxy.v1/MsgPluginExecute");
    assert_eq!(wasm_events.count(), 1);
}

#[test]
fn refillable_task_on_cronkitty_refills() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);

    // get the watermark for task refill
    let watermark = get_require_fund(TASK_GAS_LIMIT);
    let (task_on_croncat, proxy, _) =
        mock_setup_a_task(&mut suite, &cc_contracts, Some(Uint128::from(watermark)));
    //mock_setup_a_task(&mut suite, &cc_contracts, Some(Uint128::from(watermark)));

    let pre_task_balance_on_croncat = query_task_balance(
        &mut suite,
        &cc_contracts.manager,
        task_on_croncat.task_hash.clone(),
    )
    .balance
    .unwrap();

    suite.fast_forward_block_time(10000);
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    suite
        .app
        .execute_contract(
            Addr::unchecked(AGENT),
            cc_contracts.manager.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap();

    let after_one_execute_proxy_balance = suite.query_balance(&proxy).unwrap();
    let after_one_task_balance_on_croncat = query_task_balance(
        &mut suite,
        &cc_contracts.manager,
        task_on_croncat.task_hash.clone(),
    )
    .balance
    .unwrap();

    suite.fast_forward_block_time(10000);
    let proxy_call_msg = ManagerExecuteMsg::ProxyCall { task_hash: None };
    let res = suite
        .app
        .execute_contract(
            Addr::unchecked(AGENT),
            cc_contracts.manager.clone(),
            &proxy_call_msg,
            &vec![],
        )
        .unwrap();

    // We must check that second task also happened
    let wasm_events = res
        .events
        .iter()
        .filter(|event| event.ty == "wasm-vectis.proxy.v1/MsgPluginExecute");
    assert_eq!(wasm_events.count(), 1);

    let after_two_execute_proxy_balance = suite.query_balance(&proxy).unwrap();
    let after_two_task_balance_on_croncat =
        query_task_balance(&mut suite, &cc_contracts.manager, task_on_croncat.task_hash)
            .balance
            .unwrap();

    assert_eq!(
        after_one_task_balance_on_croncat,
        after_two_task_balance_on_croncat
    );

    assert_eq!(
        // The first task will deduct without refill
        pre_task_balance_on_croncat
            .native_balance
            .checked_sub(after_one_task_balance_on_croncat.native_balance)
            .unwrap(),
        // refill happens after the first balance
        after_one_execute_proxy_balance
            .amount
            .checked_sub(after_two_execute_proxy_balance.amount)
            .unwrap()
            // This is because the task set sends 1
            .checked_sub(Uint128::one())
            .unwrap()
    )
}

#[test]
fn plugin_info_is_correct() {
    let mut suite = HubChainSuite::init().unwrap();
    let cc_contracts =
        setup_croncat_contracts(&mut suite.app, &suite.deployer_signer, &suite.controller);
    let (_, _, cronkitty) = mock_setup_a_task(&mut suite, &cc_contracts, None);
    let plugin_info = suite.query_plugin_info(&cronkitty).unwrap();
    assert_eq!(plugin_info.contract_version, VECTIS_VERSION);
    let version_details = plugin_info
        .plugin_info
        .get_version_details(VECTIS_VERSION)
        .unwrap();
    assert_eq!(&version_details.ipfs_hash, "some-hash")
}
