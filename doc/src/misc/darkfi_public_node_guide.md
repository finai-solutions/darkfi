# DarkFi public node guide

Public nodes are nodes that reveal themselves to the network. They are publicly
accessible so they can accept inbound connections from other nodes.
Public nodes are important for the health of the DarkFi P2P network. This guide 
explains how to run the optimal DarkFi public node configurations.

<u><b>Note</b></u>: If you do not want your IP address to be public you can run 
a node using Tor.

<u><b>Note</b></u>: This page is a general guide for public nodes in the DarkFi 
ecosystem and is applicable to other apps such as `taud` and `darkfid`. We use 
`darkirc` as our main example throughout this guide. Commands such as `./darkirc`
and configuration filenames need to be adjusted if using different apps.
If you're using another app, the network configurations remain the same except 
for the seed nodes you connect to. 

## Generating configuration files

After compiling, you can start the application so it can spawn its configuration 
file. We use `darkirc` as the application example going forward.

```shell
% ./darkirc
```

`darkirc` creates a configuration file `darkirc_config.toml` by default in 
`~/.config/darkfi/`. You will review and edit this configuration file for your 
preferred network settings. 

## Configure your network settings

Edit your `darkirc_config.toml` file to reflect the network settings you want 
to support. Listed below are different `darkirc_config.toml` configurations. You 
can choose between a clearnet node, a fully anonymous Tor node, or a bridge 
node which runs over clearnet & Tor, and is most beneficial for the health of 
the network.

<u><b>Note</b></u>: As you modify the file, if you notice some settings are missing, 
simply add them. Some settings may be commented-out by default. In the example 
configurations below, you will find the placeholders `MY_IP_V4`, `MY_IP_V6`, 
`my.resolveable.address`, and `youraddress.onion` which indicates you should replace 
them with your public IPv4, IPv6, domain or your onion address. If you don't 
have some of them (for example: IPv6 or domain) remove the values entirely.

### Clearnet node

A clearnet node routes traffic over `tcp+tls`. You can find a complete example config 
file for `darkirc-clearnet.toml` in `${DARKFI_REPO}/bin/darkirc/config`.

```toml
## Whitelisted transports for outbound connections
allowed_transports = ["tcp+tls"]

## Addresses we want to advertise to peers
external_addrs = ["tcp+tls://MY_IP_V4:26661", "tcp+tls://MY_IP_V6:26661", "tcp+tls://my.resolveable.address:26661"]

## Seed nodes to connect to 
seeds = ["tcp+tls://lilith1.dark.fi:5262"]

## P2P accept addresses
inbound = ["tcp+tls://0.0.0.0:26661", "tcp+tls://[::]:26661"]

## Outbound connection slots
outbound_connections = 8

## Inbound connection slots
inbound_connections = 64

## Enable transport mixing
transport_mixing = false
```

### Fully anonymous Tor-enabled node

A Tor-enabled node routes traffic over `Tor`. You can find a complete example config 
file for `darkirc-tor.toml` in `${DARKFI_REPO}/bin/darkirc/config`. This node 
configuration is for users that would like to support `darkirc` over the Tor 
network. A Tor node provides the best anonymity on the network.

You need to configure Tor and launch your hidden service prior to running your 
public node over Tor. Please refer to 
[Tor DarkIRC](tor_darkirc.md#step-1-install-tor-and-launch-a-hidden-service).

```toml
## connection settings
outbound_connect_timeout = 60
channel_handshake_timeout = 55
channel_heartbeat_interval = 90
outbound_peer_discovery_cooloff_time = 60

## Whitelisted transports for outbound connections
allowed_transports = ["tor", "tor+tls"]

## Addresses we want to advertise to peers
external_addrs = ["tor://youraddress.onion:25551"]

## Seed nodes to connect to 
seeds = [
    "tor://czzulj66rr5kq3uhidzn7fh4qvt3vaxaoldukuxnl5vipayuj7obo7id.onion:5263",
    "tor://vgbfkcu5hcnlnwd2lz26nfoa6g6quciyxwbftm6ivvrx74yvv5jnaoid.onion:5273",
]

## P2P accept addresses
inbound = ["tcp://127.0.0.1:25551"]

## Outbound connection slots
outbound_connections = 8

## Inbound connection slots
inbound_connections = 64

## Enable transport mixing
transport_mixing = false
```

### Bridge node

A bridge node is a node that offers connectivity via multiple transport layers. 
This provides the most benefit for the health of the network. This is the most 
maximally compatible node for people that wish to support the network. You can 
find a complete example config file for `darkirc-mixed.toml` in 
`${DARKFI_REPO}/bin/darkirc/config`. Refer to 
[Tor DarkIRC](tor_darkirc.md#step-1-install-tor-and-launch-a-hidden-service)
to configure Tor.

```toml
## connection settings
outbound_connect_timeout = 60
channel_handshake_timeout = 55
channel_heartbeat_interval = 90
outbound_peer_discovery_cooloff_time = 60

## Whitelisted transports for outbound connections
allowed_transports = ["tcp+tls", "tor"]

## Addresses we want to advertise to peers
external_addrs = ["tcp+tls://MY_IP_V4:26661", "tcp+tls://MY_IP_V6:26661", "tcp+tls://my.resolveable.address:26661", "tor://youraddress.onion:25551"]

## Seed nodes to connect to 
seeds = [
    "tcp+tls://lilith1.dark.fi:5262",
    "tor://czzulj66rr5kq3uhidzn7fh4qvt3vaxaoldukuxnl5vipayuj7obo7id.onion:5263",
    "tor://vgbfkcu5hcnlnwd2lz26nfoa6g6quciyxwbftm6ivvrx74yvv5jnaoid.onion:5273",
]

## P2P accept addresses
inbound = ["tcp://127.0.0.1:25551", "tcp+tls://0.0.0.0:26661", "tcp+tls://[::]:26661"]

## Outbound connection slots
outbound_connections = 8

## Inbound connection slots
inbound_connections = 64

## Enable transport mixing
transport_mixing = true
```

## Test your node

You can test if your node is configured properly on the network. Use 
[Dnet](../learn/dchat/network-tools/using-dnet.md) and the
[ping-tool](network-troubleshooting.md#ping-tool) to test your node
connections. You can view if your node is making inbound and outbound connections.

## Troubleshooting

Refer to [Network troubleshooting](network-troubleshooting.md)
for further troubleshooting resources.