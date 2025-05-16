file="./main.tx3"

[[wallets]]
name = "bob"
balance = 10000000

[[wallets]]
name = "alice"
balance = 5000000

[[transactions]]
description = "bob sends 2 ada to alice"
template = "transfer"
signers = "bob"
args = { quantity = 2000000, sender = "@bob", receiver = "@alice" }

[[transactions]]
description = "alice sends 2 ada to bob"
template = "transfer"
signers = "alice"
args = { quantity = 2000000, sender = "@alice", receiver = "@bob" }

[[expect]]
type = "Balance"
wallet = "bob"
amount = 9638899

[[expect]]
type = "Balance"
wallet = "alice"
amount = { target = 4638899, threshold = 300000 } 
