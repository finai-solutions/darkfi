from darkfi_eventgraph_py import event_graph as eg, p2p, sled
import asyncio
import random
import time
import subprocess
import os
import threading
# number of nodes
N = 1
P2PDATASTORE_PATH = '/tmp/p2pdatastore'
SLED_DB_PATH = '/tmp/sleddb'
STARTING_PORT = 54321
os.system("rm -rf " + P2PDATASTORE_PATH+"*")
os.system("rm -rf " + SLED_DB_PATH+"*")
OUTBOUND_TIMEOUT = 2
CH_HANDSHAKE_TIMEOUT = 15
CH_HEARTBEAT_INTERVAL = 15
DISCOVERY_COOLOFF = 15
DISCOVERY_ATTEMPT = 5
REFINERY_INTERVAL = 15
WHITE_CONNECT_PERCENT = 70
GOLD_CONNECT_COUNT = 2
TIME_NO_CON = 60
W8_TIME = 60


async def is_connected_async(node):
    return await p2p.is_connected(node)

def is_connected(node):
    return asyncio.run(is_connected_async(node))

async def get_greylist_length(node):
    return await p2p.get_greylist_length(node)

def get_random_node_idx():
    return 0

async def start_p2p(w8_time, node):
    await p2p.start_p2p(w8_time, node)

async def get_fut_p2p(settings):
    return await p2p.new_p2p(settings)

async def get_fut_eg(node, sled_db):
    return await eg.new_event_graph(node, sled_db, P2PDATASTORE_PATH, False, 'dag', 1)

async def register_protocol(p2p_node, eg_node):
    await p2p.register_protocol_p2p(p2p_node, eg_node)

def new_nodes(starting_port=STARTING_PORT):
    p2ps = []
    event_graphs = []
    # for i in range(1, N):

    inbound_port = starting_port + 1
    external_port = starting_port + 1
    node_id = str(inbound_port)
    addrs = p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))
    inbound_addrs = [addrs]
    external_addrs = [addrs]
    peers = []
    seeds = [p2p.Url("tcp://127.0.0.1:25551")]
    app_version = p2p.new_version(0, 5, 1, '')
    allowed_transports = ['tcp']
    transport_mixing = False
    outbound_connections = 4
    inbound_connections = 0
    outbound_connect_timeout = OUTBOUND_TIMEOUT
    channel_handshake_timeout = CH_HANDSHAKE_TIMEOUT
    channel_heartbeat_interval = CH_HEARTBEAT_INTERVAL
    localnet = True
    outbound_peer_discovery_cooloff_time = DISCOVERY_COOLOFF
    outbound_peer_discovery_attempt_time = DISCOVERY_ATTEMPT
    p2p_datastore = P2PDATASTORE_PATH+'{}'.format(0)
    hostlist = ''
    greylist_refinery_internval = REFINERY_INTERVAL
    white_connect_percnet = WHITE_CONNECT_PERCENT
    gold_connect_count = GOLD_CONNECT_COUNT
    slot_preference_strict = False
    time_with_no_connections = TIME_NO_CON
    blacklist = []
    ban_policy = p2p.get_strict_banpolicy()
    settings = p2p.new_settings(
        node_id,
        inbound_addrs,
        external_addrs,
        peers,
        seeds,
        app_version,
        allowed_transports,
        transport_mixing,
        outbound_connections,
        inbound_connections,
        outbound_connect_timeout,
        channel_handshake_timeout,
        channel_heartbeat_interval,
        localnet,
        outbound_peer_discovery_cooloff_time,
        outbound_peer_discovery_attempt_time,
        p2p_datastore,
        hostlist,
        greylist_refinery_internval,
        white_connect_percnet,
        gold_connect_count,
        slot_preference_strict,
        time_with_no_connections,
        blacklist,
        ban_policy
    )
    p2p_ptr = asyncio.run(get_fut_p2p(settings))
    sled_db = sled.SledDb(SLED_DB_PATH+'{}'.format(1))
    event_graph = asyncio.run(get_fut_eg(p2p_ptr, sled_db))
    event_graphs+=[event_graph]
    p2ps+=[p2p_ptr]
    return (p2ps, event_graphs)

async def create_new_event(data, event_graph_ptr):
    return await eg.new_event(data, event_graph_ptr)

async def insert_events(node, event):
    ids = await node.dag_insert(event)
    return ids

async def broadcast_event_onp2p(w8_time, p2p_node, event):
    await p2p.broadcast_p2p(w8_time, p2p_node, event)

async def get_event_by_id(event_graph, event_id):
    return await event_graph.dag_get(event_id)

async def dag_sync(node):
    await node.dag_sync()


############################
#       create seed        #
############################
# seed_p2p_ptr, seed_addr, seed_event_graph = get_seed_node()
start_ts = []
# seed_t = threading.Thread(target=asyncio.run, args=(start_p2p(W8_TIME, seed_p2p_ptr),))
# seed_t.start()
# start_ts += [seed_t]
# seed_register_t = threading.Thread(target=asyncio.run, args=(register_protocol(seed_p2p_ptr, seed_event_graph),))
# seed_register_t.start()
############################
#      create N nodes      #
############################
p2ps, egs = new_nodes()

############################
#     register node        #
############################
register_ts = []
for idx, node in enumerate(p2ps):
    # register event graph protocol
    eg_t = threading.Thread(target=asyncio.run, args=(register_protocol(node, egs[idx]),))
    eg_t.start()
    register_ts += [eg_t]
for t in register_ts:
    t.join()

###########################
#     start node          #
###########################

for node in p2ps:
    # start p2p node
    node_t = threading.Thread(target=asyncio.run, args=(start_p2p(W8_TIME, node),))
    node_t.start()
    start_ts += [node_t]

# select random node
rnd_idx = get_random_node_idx()
random_node = egs[rnd_idx]

for evg in egs:
    len_before_broadcast = evg.dag_len()
    print("len_before_broadcast: {}".format(len_before_broadcast))
    assert(evg.dag_len()==1)

# create new event                    This msg is serialization of "#dev Alice :hello"  
event = asyncio.run(create_new_event([4, 35, 100, 101, 118, 5, 65, 108, 105, 99, 101, 5, 104, 101, 108, 108, 111], random_node))

# insert event at random node
# ids = asyncio.run(insert_events(random_node, [event]))

# wait for nodes to conenct
time.sleep(40)

for node in p2ps:
    assert(is_connected(node))
    print('node: {} is connected successfully'.format(node))

dag_ts = []

print("=======================")
print("=      dag sync       =")
print("=======================")
# dag sync
for eg in egs:
    dag_t = threading.Thread(target=asyncio.run, args=(dag_sync(eg),))
    dag_t.start()
    dag_ts+=[dag_t]

for t in dag_ts:
    t.join()

# Sync first then broadcast
# broadcast the new event
random_node_p2p = p2ps[rnd_idx]

threading.Thread(target=asyncio.run, args=(broadcast_event_onp2p(15, random_node_p2p, event),)).start()

# get broadcasted event
# event2 = asyncio.run(get_event_by_id(egs[rnd_idx], ids[0]))
# print("broadcasted event: {}".format(event2))

time.sleep(30)
# assert event is broadcast to all nodes
for evg in  egs:
    len_after_broadcast = evg.dag_len()
    print("len_after_broadcast: {}".format(len_after_broadcast))
    assert(len_after_broadcast > len_before_broadcast)
for t in start_ts:
    t.join()
