create a todo.md and add to find legacy versions of contracts and add these in as separate contracts in the config, as if they were not connected, and process them into the same tables as similar
contracts but with an added column which is the contract address that is being queried

add a measure of skipped rounds to the graph
maybe add something while processes the data and produces an overview of each round

add data to satisfy

SELECT block_number, CAST(withdraw_amount AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'PotWithdrawn'
V
in a feature branch, using a new database beeport_test_2, add the retrieval and processing of the PotWithdrawn event from the blockchain in line with the rest of the project, if the command to refresh just the postagestamp contract is already present, tell me what command to run to update a database of events from the version  before you added the potwitdrawn event. if there is not a command which will refresh just the postagestamp contract, please add one

add to a hook a
  dummy example of sending a transaction to a custom RPC using a private key stored in an environment variable.
