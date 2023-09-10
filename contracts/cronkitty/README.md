# Cronkitty - meow

> Special thanks to @mikedotexe and the Croncat team for review and feedback in the development of this plugin

Cronkitty is a wrapper around the [Croncat contracts](https://github.com/CronCats/cw-croncat).

The benefit of adding the Cronkitty plugin,
as opposed to using Vectis Extension / Vectis Account to directly set tasks on Croncat is so that:

- **Auto Gas Refill**: No need to worry about task on CronCat run out of gas, you can set a task balance such that if the balance on CronCat goes below, your wallet will auto-refill it
- **Preserves origin**: `info.sender` is always the Vectis Account: target application does not see the difference in a manual / automated transaction
- **Self custody**: Funds stay in the Vectis Account: setting reoccuring transactions does not require funds to be provided in advance

## Design

The CronCat code has comprehensive documentation, please go to https://docs.cron.cat/ for details.

For the sake of understanding cronkitty, you need to know that

- Croncat Manager calls for execute, stores task balances
- Croncat Tasks handles creating

In CronKitty, we create a `action_id` that is stored with the desired `CosmosMsgs` to be executed,
along with some metadata to keep track of the versioning of the Croncat contracts and the auto-refill option.

The actual task created on CronCat is simply as a call back to cronkitty via the `execute` entry point with the relevant `action_id`,
who will then call the Vectis Account with the desired `CosmosMsgs`.

### Auto refill

CronCat's has a set of base and added gas fees to support its operations.

Instead of doing the same simulation to know how much to refill the task balance on the CronCat Manager contract,
we know that there are minimum gas for 2 tasks, which is guaranteed by the `multiplier` used when creating a task in `TaskBalance`.

Every time `execute` on cronkitty is called,
we compare the task balance on the CronCat Manager and the auto-refill limit.
If the auto-refill limit is above the task balance, a message will be sent from the proxy (where the fund is) -> cronkitty (owner of the task) -> CronCat Manager
to refill the task balance.
