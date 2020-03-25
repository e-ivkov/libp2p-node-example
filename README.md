# Libp2p node exmple
## Description
Shows libp2p usage scenario as a blockchain node.
It is mainly an example to get performance stats of libp2p.
### Protocols used
1. ping
2. floodsub (pubsub implementation)
3. mdns (mainly for development purposes, to test faster on local machine)
### Stats gathered
The node gathers the following stats in a `stats.txt`, which it saves on exit.
1. Average ping time to each peer
2. Average transmission rate (through floodsub) to each peer measured in bytes per unit of time

This stats are saved only for the latest N requests, where N is specified as command line argument
`stats_window size`
### What the node does
1. Connects to the swarm on startup
2. Periodically pings other nodes
3. Periodically sends `pending_tx` to other nodes
4. Logs all the incoming messages
5. Exits after specified time saving the stats to a file

## How to deploy
1. Clone the repository
2. `cargo run -- --help` to get the list of possible parameters
3. `cargo run -- --node_addr=ADDR` to deploy and connect to the swarm through the node with `ADDR` address

## Test procedure
As initially this repository is meant as a performance testing example for libp2p,
the procedure to test and get results is the following.

All the nodes should be deployed on different machines, preferably in different networks.

1. Deploy the first node and record it's address
2. Deploy other nodes with the first node's address as a cli parameter
3. Wait for the first node to exit (exit time can be also specified in cli args)
4. Retrieve `stats.txt` from the first node's machine. It should be located near the executable file.