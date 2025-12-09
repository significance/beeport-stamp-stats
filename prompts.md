make a plan for this so i can check you have understood it completely

1. add exponential backoff to the ./target/release/beeport-stamp-stats fetch command, reuse existing backoff code. if it must be refactored in order to reuse it, do so

2. create a update command for batch_status, it should only check batches that previously had a non zero balance, and update their balance based on the current status on the blockchain.

3. add to the follow command, it must amend entries in the batch status database table to keep them up to date if some event changes their balance or depth, i.e. their normalised balance








later...

1. create a command that tracks payouts from the redistribution contract, i.e. the amount paid out in each round, and to whom
2. add the option for postgresql used as a database, as well as sqlite. make it configurable
3. create a basic restful api that serves information about the batches, lists them, filters, retrieves individual ones. make it nice and up to date