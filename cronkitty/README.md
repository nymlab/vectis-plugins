# Cronkitty - meow

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
