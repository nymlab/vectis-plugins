use cosmwasm_std::{Addr, Empty};
use croncat_sdk_factory::msg::{
    ContractMetadataResponse, FactoryExecuteMsg, ModuleInstantiateInfo, VersionKind,
};
use croncat_sdk_tasks::msg::TasksInstantiateMsg;
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use vectis_contract_tests::common::common::*;
pub const PAUSE_ADMIN: &str = "stars15434j0vvv8un4hs0sfx8avmnc7ypzg00hjyc2c5a6z352t4tjdmsj4785v";
// These addresses need to be well formed as balances are queried in croncat contract
pub const AGENT_BENEFICIARY: &str = "wasm1ucl9dulgww2trng0dmunj348vxneufu5nk4yy4";
pub const AGENT0: &str = "wasm1ucl9dulgww2trng0dmunj348vxneufu5n11yy4";
/// This is used for staking queries
/// https://github.com/CosmWasm/cosmwasm/blob/32f308a1a56ae5b8278947891306f7a374c3df94/packages/vm/src/environment.rs#L383
pub const DENOM: &str = "TOKEN";

pub(crate) fn croncat_tasks_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        croncat_tasks::contract::execute,
        croncat_tasks::contract::instantiate,
        croncat_tasks::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn croncat_factory_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        croncat_factory::contract::execute,
        croncat_factory::contract::instantiate,
        croncat_factory::contract::query,
    )
    .with_reply(croncat_factory::contract::reply);
    Box::new(contract)
}

pub(crate) fn croncat_manager_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        croncat_manager::contract::execute,
        croncat_manager::contract::instantiate,
        croncat_manager::contract::query,
    )
    .with_reply(croncat_manager::contract::reply);
    Box::new(contract)
}

pub(crate) fn croncat_agents_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        croncat_agents::contract::execute,
        croncat_agents::contract::instantiate,
        croncat_agents::contract::query,
    );
    Box::new(contract)
}
pub(crate) fn init_factory(app: &mut App) -> Addr {
    let code_id = app.store_code(croncat_factory_contract());
    app.instantiate_contract(
        code_id,
        Addr::unchecked("deployer"),
        &croncat_factory::msg::InstantiateMsg { owner_addr: None },
        &[],
        "croncat_factory",
        None,
    )
    .unwrap()
}

// Below init functions copied from the croncat repo
pub(crate) fn init_tasks(app: &mut App, msg: &TasksInstantiateMsg, factory_addr: &Addr) -> Addr {
    let code_id = app.store_code(croncat_tasks_contract());
    let module_instantiate_info = ModuleInstantiateInfo {
        code_id,
        version: [0, 1],
        commit_id: "commit1".to_owned(),
        checksum: "checksum2".to_owned(),
        changelog_url: None,
        schema: None,
        msg: to_binary(msg).unwrap(),
        contract_name: "tasks".to_owned(),
    };
    app.execute_contract(
        Addr::unchecked("deployer"),
        factory_addr.to_owned(),
        &FactoryExecuteMsg::Deploy {
            kind: VersionKind::Tasks,
            module_instantiate_info,
        },
        &[],
    )
    .unwrap();

    let metadata: ContractMetadataResponse = app
        .wrap()
        .query_wasm_smart(
            factory_addr,
            &croncat_factory::msg::QueryMsg::LatestContract {
                contract_name: "tasks".to_owned(),
            },
        )
        .unwrap();
    metadata.metadata.unwrap().contract_addr
}

pub(crate) fn init_manager(app: &mut App, factory_addr: &Addr) -> Addr {
    let code_id = app.store_code(croncat_manager_contract());
    let msg = croncat_manager::msg::InstantiateMsg {
        version: Some("0.1".to_owned()),
        croncat_tasks_key: ("tasks".to_owned(), [0, 1]),
        croncat_agents_key: ("agents".to_owned(), [0, 1]),
        gas_price: None,
        treasury_addr: None,
        cw20_whitelist: None,
        pause_admin: Addr::unchecked(PAUSE_ADMIN),
    };
    let module_instantiate_info = ModuleInstantiateInfo {
        code_id,
        version: [0, 1],
        commit_id: "commit1".to_owned(),
        checksum: "checksum2".to_owned(),
        changelog_url: None,
        schema: None,
        msg: to_binary(&msg).unwrap(),
        contract_name: "manager".to_owned(),
    };
    app.execute_contract(
        Addr::unchecked("deployer"),
        factory_addr.to_owned(),
        &FactoryExecuteMsg::Deploy {
            kind: VersionKind::Tasks,
            module_instantiate_info,
        },
        &[],
    )
    .unwrap();

    let metadata: ContractMetadataResponse = app
        .wrap()
        .query_wasm_smart(
            factory_addr,
            &croncat_factory::msg::QueryMsg::LatestContract {
                contract_name: "manager".to_owned(),
            },
        )
        .unwrap();
    metadata.metadata.unwrap().contract_addr
}

pub(crate) fn init_agents(app: &mut App, factory_addr: &Addr) -> Addr {
    let code_id = app.store_code(croncat_agents_contract());
    let msg = croncat_agents::msg::InstantiateMsg {
        version: Some("0.1".to_owned()),
        croncat_manager_key: ("manager".to_string(), [0, 1]),
        croncat_tasks_key: ("tasks".to_string(), [0, 1]),
        agent_nomination_duration: None,
        min_tasks_per_agent: None,
        agents_eject_threshold: None,
        min_active_agent_count: None,
        pause_admin: Addr::unchecked(PAUSE_ADMIN),
        public_registration: true,
        allowed_agents: None,
        min_coins_for_agent_registration: None,
    };
    let module_instantiate_info = ModuleInstantiateInfo {
        code_id,
        version: [0, 1],
        commit_id: "commit1".to_owned(),
        checksum: "checksum2".to_owned(),
        changelog_url: None,
        schema: None,
        msg: to_binary(&msg).unwrap(),
        contract_name: "agents".to_owned(),
    };
    app.execute_contract(
        Addr::unchecked("deployer"),
        factory_addr.to_owned(),
        &FactoryExecuteMsg::Deploy {
            kind: VersionKind::Tasks,
            module_instantiate_info,
        },
        &[],
    )
    .unwrap();

    let metadata: ContractMetadataResponse = app
        .wrap()
        .query_wasm_smart(
            factory_addr,
            &croncat_factory::msg::QueryMsg::LatestContract {
                contract_name: "agents".to_owned(),
            },
        )
        .unwrap();
    metadata.metadata.unwrap().contract_addr
}
pub(crate) fn default_instantiate_msg() -> TasksInstantiateMsg {
    TasksInstantiateMsg {
        chain_name: "atom".to_owned(),
        version: Some("0.1".to_owned()),
        croncat_manager_key: ("manager".to_owned(), [0, 1]),
        croncat_agents_key: ("agents".to_owned(), [0, 1]),
        slot_granularity_time: None,
        gas_base_fee: None,
        gas_action_fee: None,
        gas_query_fee: None,
        gas_limit: None,
        pause_admin: Addr::unchecked(PAUSE_ADMIN),
    }
}
