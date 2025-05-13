# TX3 test

Trix implements a tool to help test tx3 files. Follow the documentation to read more about the test data structure. Trix spaw a devnet and configure wallets with initial values, so the transaction scenarios are executed and the expect definition needs to match.

## How to run

With cargo running the source

```sh
cargo run -- test ./tests/basic.toml
```

With trix binary

```sh
trix test ./tests/basic.toml
```

> **ℹ️ Info**
> 
> Each transaction will have fees, so for the balance expect, consider the fees.
