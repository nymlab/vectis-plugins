# Cronkitty

[Demo video with Vectis Smart Account](https://youtu.be/QTN-OOld80w?si=i5nabMIRaSo4KEkY)

> Special thanks to @mikedotexe and the Croncat team for review and feedback in the development of this plugin

Cronkitty is a wrapper around the [Croncat contracts](https://github.com/CronCats/cw-croncat).

The benefit of adding the Cronkitty plugin,
as opposed to using Vectis Extension / Vectis Account to directly set tasks on Croncat is so that:

- `info.sender` is always the Vectis Account: target application does not see the difference in a manual / automated transaction
- Funds stay in the Vectis Account: setting reoccuring transactions does not require funds to be provided in advance

## Design

The Croncat system is comprehensive, for the sake of understanding the code, you need to know that

- Croncat Manager calls for execute
- Croncat Tasks handles creating

In CronKitty, we create a `action_id` that is stored with the desired `CosmosMsgs` to be executed,
along with some metadata to keep track of the versioning of the Croncat contracts.

The actual task created on Croncat is for the agents to call the `execute` entry point in the Cronkitty contract,
who will then call the Vectis Account with the desired `CosmosMsgs`.


