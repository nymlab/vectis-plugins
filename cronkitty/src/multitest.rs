use anyhow::{bail, Result as AnyResult};
use cosmwasm_std::{from_slice, Empty};
use cw_multi_test::Contract;

use crate::{
    contract::{ContractExecMsg, ContractQueryMsg, CronKittyPlugin, InstantiateMsg, MigrateMsg},
    reply,
};

impl Contract<Empty> for CronKittyPlugin<'_> {
    fn execute(
        &self,
        deps: cosmwasm_std::DepsMut<Empty>,
        env: cosmwasm_std::Env,
        info: cosmwasm_std::MessageInfo,
        msg: Vec<u8>,
    ) -> AnyResult<cosmwasm_std::Response<Empty>> {
        from_slice::<ContractExecMsg>(&msg)?
            .dispatch(self, (deps, env, info))
            .map_err(Into::into)
    }

    fn instantiate(
        &self,
        deps: cosmwasm_std::DepsMut<Empty>,
        env: cosmwasm_std::Env,
        info: cosmwasm_std::MessageInfo,
        msg: Vec<u8>,
    ) -> AnyResult<cosmwasm_std::Response<Empty>> {
        from_slice::<InstantiateMsg>(&msg)?
            .dispatch(self, (deps, env, info))
            .map_err(Into::into)
    }

    fn query(
        &self,
        deps: cosmwasm_std::Deps<Empty>,
        env: cosmwasm_std::Env,
        msg: Vec<u8>,
    ) -> AnyResult<cosmwasm_std::Binary> {
        from_slice::<ContractQueryMsg>(&msg)?
            .dispatch(self, (deps, env))
            .map_err(Into::into)
    }

    fn sudo(
        &self,
        _deps: cosmwasm_std::DepsMut<Empty>,
        _env: cosmwasm_std::Env,
        _msg: Vec<u8>,
    ) -> AnyResult<cosmwasm_std::Response<Empty>> {
        bail!("sudo not implemented for contract")
    }

    fn reply(
        &self,
        deps: cosmwasm_std::DepsMut<Empty>,
        env: cosmwasm_std::Env,
        msg: cosmwasm_std::Reply,
    ) -> AnyResult<cosmwasm_std::Response<Empty>> {
        reply(deps, env, msg).map_err(Into::into)
    }

    fn migrate(
        &self,
        deps: cosmwasm_std::DepsMut<Empty>,
        env: cosmwasm_std::Env,
        msg: Vec<u8>,
    ) -> AnyResult<cosmwasm_std::Response<Empty>> {
        from_slice::<MigrateMsg>(&msg)?
            .dispatch(self, (deps, env))
            .map_err(Into::into)
    }
}
