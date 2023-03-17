use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

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
    NoCronCatContract {
        name: String
    },
}
