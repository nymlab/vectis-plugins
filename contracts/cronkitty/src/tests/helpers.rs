use cosmwasm_std::{Addr, Empty};
use croncat_sdk_factory::msg::{ContractMetadataResponse, ModuleInstantiateInfo, VersionKind};

use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use vectis_contract_tests::common::common::*;
// These addresses need to be well formed as balances are queried in croncat contract
pub const AGENT_BENEFICIARY: &str = "wasm1ucl9dulgww2trng0dmunj348vxneufu5nk4yy4";
