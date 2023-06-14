use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, CosmosMsg, Uint128};

use crate::error::ContractError;

/// task / mgr version on croncat, msgs to execute, task_hash, auto_refill
#[cw_serde]
pub struct ActionRef {
    pub version: [u8; 2],
    pub msgs: Vec<CosmosMsg>,
    pub task_hash: Option<String>,
    pub refill_opt: Option<AutoRefill>,
    pub refill_accounting: Option<RefillAccounting>,
}

impl ActionRef {
    pub fn new(version: [u8; 2], msgs: Vec<CosmosMsg>, refill_opt: Option<AutoRefill>) -> Self {
        let refill_accounting = if refill_opt.is_some() {
            Some(RefillAccounting::default())
        } else {
            None
        };

        Self {
            version,
            msgs,
            task_hash: None,
            refill_opt,
            refill_accounting,
        }
    }

    // Checks if this task should be refilled
    pub fn refillable(&self) -> bool {
        if let Some(refill_opt) = self.refill_opt.as_ref() {
            if let Some(refill_limit) = refill_opt.stop_after.as_ref() {
                // if ActionRef has refill_opt, it will always have refill_accounting
                match refill_limit {
                    RefillLimit::FailedAttempts(attempts_limit) => {
                        &self.refill_accounting.as_ref().unwrap().total_action_failed
                            < attempts_limit
                    }
                    RefillLimit::TotalRefilledAmount(amounts_limit) => self
                        .refill_accounting
                        .as_ref()
                        .unwrap()
                        .total_refill_amount
                        .lt(amounts_limit),
                }
            } else {
                true
            }
        } else {
            false
        }
    }

    pub fn get_watermark(&self) -> Option<Uint128> {
        self.refill_opt.as_ref().and_then(|opt| opt.trigger_balance)
    }

    pub fn refill(&mut self, refill_amount: Uint128) -> Result<(), ContractError> {
        match self.refill_accounting.as_mut() {
            Some(acct) => {
                acct.last_refill_amount = refill_amount;
                acct.total_refill_amount = acct
                    .total_refill_amount
                    .checked_add(refill_amount)
                    .map_err(|_| ContractError::Overflow)?;
                acct.total_refills += 1;
                Ok(())
            }
            None => Err(ContractError::NotRefillable),
        }
    }

    pub fn failed_action(mut self) -> Result<Self, ContractError> {
        if let Some(acct) = self.refill_accounting.as_mut() {
            acct.total_action_failed += 1;
        }
        Ok(self)
    }
}

#[cw_serde]
pub struct CronKittyActionResp {
    pub msgs: Vec<CosmosMsg>,
    pub task_hash: Option<String>,
    pub task_addr: Addr,
    pub manager_addr: Addr,
    pub auto_refill: Option<AutoRefill>,
    pub refill_accounting: Option<RefillAccounting>,
}

#[derive(Default)]
#[cw_serde]
pub struct RefillAccounting {
    pub last_refill_amount: Uint128,
    pub total_action_failed: u64,
    pub total_refills: u64,
    pub total_refill_amount: Uint128,
}

/// This supports auto-refilling of croncat tasks directly from the
#[cw_serde]
pub struct AutoRefill {
    /// AutoRefill limit, if unset, it will continue until proxy wallet has no funds left to
    /// refill, at which point, the task will continue with the remaining balance on the Croncat
    /// contract
    pub stop_after: Option<RefillLimit>,
    /// Balance under which the refill is triggered,
    /// if it is not set, it will be the initial balance at task creation
    pub trigger_balance: Option<Uint128>,
}

#[cw_serde]
pub enum RefillLimit {
    FailedAttempts(u64),
    TotalRefilledAmount(Uint128),
}

#[cfg(test)]
mod test {
    const VERSION: [u8; 2] = [1, 1];
    use super::*;

    #[test]
    pub fn non_refillable_works() {
        let action_ref = ActionRef::new(VERSION, vec![], None);
        assert!(!action_ref.refillable())
    }
}
