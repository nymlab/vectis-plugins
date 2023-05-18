use cosmwasm_std::StdError;
use cw_utils::ParseReplyError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Empty Funds")]
    EmptyFunds,

    #[error("Overflow")]
    Overflow,

    #[error("InvalidReplyId")]
    InvalidReplyId,

    #[error("Insufficient funds for Task")]
    NotEnoughFundsForGas,

    #[error("Expected event not found")]
    ExpectedEventNotFound,

    #[error("Task hash not found")]
    TaskHashNotFound,

    #[error("Did not find CronCat contract {name} from factory")]
    NoCronCatContract { name: String },

    #[error("Did not find CronCat latest version for {name} from factory")]
    NoCronCatVersion { name: String },

    #[error("Cron cat reply is not expected")]
    UnexpectedCroncatTaskReply,

    #[error("Croncat task hash is not the one saved")]
    UnexpectedCroncatTaskHash,
}
