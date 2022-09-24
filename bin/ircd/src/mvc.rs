use async_std::sync::{Arc, Mutex};
use std::{
    collections::{HashMap, HashSet},
    fmt, io,
};

use async_executor::Executor;
use async_recursion::async_recursion;
use ripemd::{Digest, Ripemd256};
use smol::future;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    serial::{Decodable, Encodable, ReadExt, SerialDecodable, SerialEncodable},
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::{expand_path, get_config_path},
    },
    Result,
};

// TODO
// Pass EventId for all the functions instead of EventNodePtr
// Use hashmap for Event's childrens
// Remove Mutex from Event's childrens
// More tests

type EventId = [u8; 32];

const MAX_DEPTH: u32 = 10;

#[derive(SerialEncodable, SerialDecodable)]
struct Event {
    previous_event_hash: EventId,
    action: EventAction,
    timestamp: u64,
}

impl Event {
    fn hash(&self) -> EventId {
        let mut bytes = Vec::new();
        self.encode(&mut bytes).expect("serialize failed!");

        let mut hasher = Ripemd256::new();
        hasher.update(bytes);
        let bytes = hasher.finalize().to_vec();
        let mut result = [0u8; 32];
        result.copy_from_slice(bytes.as_slice());
        result
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.action {
            EventAction::PrivMsg(event) => {
                write!(f, "PRIVMSG {}: {} ({})", event.nick, event.msg, self.timestamp)
            }
        }
    }
}

enum EventAction {
    PrivMsg(PrivMsgEvent),
}

impl Encodable for EventAction {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        match self {
            Self::PrivMsg(event) => {
                let mut len = 0;
                len += 0u8.encode(&mut s)?;
                len += event.encode(s)?;
                Ok(len)
            }
        }
    }
}

impl Decodable for EventAction {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let type_id = d.read_u8()?;
        match type_id {
            0 => Ok(Self::PrivMsg(PrivMsgEvent::decode(d)?)),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Bad type ID byte for Event")),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable)]
struct PrivMsgEvent {
    nick: String,
    msg: String,
}

#[derive(Debug)]
struct EventNode {
    // Only current root has this set to None
    parent: Option<EventNodePtr>,
    event: Event,
    children: Mutex<Vec<EventNodePtr>>,
}

type EventNodePtr = Arc<EventNode>;

#[derive(Debug)]
struct Model {
    // This is periodically updated so we discard old nodes
    current_root: EventId,
    orphans: Vec<Event>,
    event_map: HashMap<EventId, EventNodePtr>,
}

impl Model {
    fn new() -> Self {
        let root_node = Arc::new(EventNode {
            parent: None,
            event: Event {
                previous_event_hash: [0u8; 32],
                action: EventAction::PrivMsg(PrivMsgEvent {
                    nick: "root".to_string(),
                    msg: "Let there be dark".to_string(),
                }),
                timestamp: get_current_time(),
            },
            children: Mutex::new(Vec::new()),
        });
        let root_node_id = root_node.event.hash();

        let event_map = HashMap::from([(root_node_id.clone(), root_node)]);

        Self { current_root: root_node_id, orphans: Vec::new(), event_map }
    }

    async fn add(&mut self, event: Event) {
        self.orphans.push(event);
        self.reorganize().await;
    }

    // TODO: Update root only after some time
    // Recursively free nodes climbing up from old root to new root
    // Also remove entries from event_map

    async fn reorganize(&mut self) {
        let mut remaining_orphans = Vec::new();
        for orphan in std::mem::take(&mut self.orphans) {
            let prev_event = orphan.previous_event_hash.clone();

            // clean up the tree from old eventnodes
            self.prune_forks().await;
            self.update_root().await;

            // Parent does not yet exist
            if !self.event_map.contains_key(&prev_event) {
                remaining_orphans.push(orphan);

                // BIGTODO #1:
                // TODO: We need to fetch missing ancestors from the network
                // Trigger get_blocks() request

                continue
            }

            let parent = self.event_map.get(&prev_event).expect("logic error").clone();
            let node = Arc::new(EventNode {
                parent: Some(parent.clone()),
                event: orphan,
                children: Mutex::new(Vec::new()),
            });

            // Reject events which attach to forks too low in the chain
            // At some point we ignore all events from old branches
            let depth = self.diff_depth(node.clone(), self.find_head().await);
            if depth > MAX_DEPTH {
                continue
            }

            parent.children.lock().await.push(node.clone());
            // Add node to the table
            self.event_map.insert(node.event.hash(), node);
        }
    }

    async fn prune_forks(&mut self) {
        let head = self.find_head().await;
        let head_hash = head.event.hash();
        for (event_hash, node) in self.event_map.clone() {
            // to prevent running through the same node twice
            if !self.event_map.contains_key(&event_hash) {
                continue
            }

            // skip the head event
            if event_hash == head_hash {
                continue
            }

            // check if the node is a leaf
            if node.children.lock().await.is_empty() {
                let depth = self.diff_depth(node.clone(), self.find_head().await);
                if depth > MAX_DEPTH {
                    self.remove_node(node.clone()).await;
                }
            }
        }
    }

    async fn update_root(&mut self) {
        let head = self.find_head().await;
        let head_hash = head.event.hash();

        // collect the leaves in the tree
        let mut leaves = vec![];

        for (event_hash, node) in self.event_map.clone() {
            // skip the head event
            if event_hash == head_hash {
                continue
            }

            // check if the node is a leaf
            if node.children.lock().await.is_empty() {
                leaves.push(node);
            }
        }

        // find the common ancestor between each leaf and the head event
        let mut ancestors = vec![];
        for leaf in leaves {
            let ancestor = self.find_ancestor(leaf, head.clone());
            let ancestor = self.event_map.get(&ancestor).unwrap().clone();
            ancestors.push(ancestor);
        }

        // find the highest ancestor
        let highest_ancestor = ancestors.iter().max_by(|&a, &b| {
            self.find_depth(a.clone(), head_hash).cmp(&self.find_depth(b.clone(), head_hash))
        });

        // set the new root
        if let Some(ancestor) = highest_ancestor {
            let ancestor_hash = ancestor.event.hash();

            // the ancestor must have at least height > 10
            let ancestor_height = self.find_height(self.get_root(), ancestor_hash).await.unwrap();
            if ancestor_height < 10 {
                return
            }

            // removing the parents of the new root node
            let mut root = self.get_root();
            loop {
                let root_hash = root.event.hash();

                if root_hash == ancestor_hash {
                    break
                }

                let root_childs = root.children.lock().await;
                assert_eq!(root_childs.len(), 1);

                let child = root_childs.get(0).unwrap().clone();
                drop(root_childs);

                self.event_map.remove(&root_hash);
                root = child;
            }

            self.current_root = ancestor_hash;
        }
    }

    async fn remove_node(&mut self, mut node: EventNodePtr) {
        loop {
            let event_id = node.event.hash();
            self.event_map.remove(&event_id);

            let parent = node.parent.as_ref().unwrap().clone();
            let parent_children = &mut parent.children.lock().await;
            let index = parent_children.iter().position(|n| n.event.hash() == event_id).unwrap();
            parent_children.remove(index);

            if !parent_children.is_empty() {
                return
            }
            node = parent.clone();
        }
    }

    fn get_root(&self) -> EventNodePtr {
        let root_id = &self.current_root;
        return self.event_map.get(root_id).expect("root ID is not in the event map!").clone()
    }

    // find_head
    // -> recursively call itself
    // -> + 1 for every recursion, return self if no children
    // -> select max from returned values
    // Gets the lead node with the maximal number of events counting from root
    async fn find_head(&self) -> EventNodePtr {
        let root = self.get_root();
        Self::find_longest_chain(root, 0).await.0
    }

    #[async_recursion]
    async fn find_longest_chain(parent_node: EventNodePtr, i: u32) -> (EventNodePtr, u32) {
        let children = parent_node.children.lock().await;
        if children.is_empty() {
            return (parent_node.clone(), i)
        }
        let mut current_max = 0;
        let mut current_node = None;
        for node in &*children {
            let (grandchild_node, grandchild_i) =
                Self::find_longest_chain(node.clone(), i + 1).await;

            if grandchild_i > current_max {
                current_max = grandchild_i;
                current_node = Some(grandchild_node.clone());
            } else if grandchild_i == current_max {
                // Break ties using the timestamp
                if grandchild_node.event.timestamp >
                    current_node.as_ref().expect("current_node should be set!").event.timestamp
                {
                    current_max = grandchild_i;
                    current_node = Some(grandchild_node.clone());
                }
            }
        }
        assert_ne!(current_max, 0);
        (current_node.expect("internal logic error"), current_max)
    }

    fn find_depth(&self, mut node: EventNodePtr, ancestor_id: EventId) -> u32 {
        let mut depth = 0;
        while node.event.hash() != ancestor_id {
            depth += 1;
            if let Some(parent) = node.parent.clone() {
                node = parent
            } else {
                break
            }
        }
        depth
    }

    #[async_recursion]
    async fn find_height(&self, parent: EventNodePtr, child_id: EventId) -> Option<u32> {
        let mut height = 0;
        if parent.event.hash() == child_id {
            return Some(height)
        }

        height += 1;

        let children = parent.children.lock().await.clone();

        if children.is_empty() {
            return None
        }

        for parent_child in children.iter() {
            if let Some(h) = self.find_height(parent_child.clone(), child_id).await {
                return Some(height + h)
            }
        }
        None
    }

    fn find_ancestor(&self, mut node_a: EventNodePtr, mut node_b: EventNodePtr) -> EventId {
        // node_a is a child of node_b
        let is_child = node_b.event.hash() == node_a.parent.as_ref().unwrap().event.hash();
        if is_child {
            return node_b.event.hash()
        }

        while node_a.event.hash() != node_b.event.hash() {
            let node_a_parent =
                node_a.parent.as_ref().expect("non-root nodes should have a parent set");

            let node_b_parent =
                node_b.parent.as_ref().expect("non-root nodes should have a parent set");

            if node_a_parent.event.hash() == self.current_root ||
                node_b_parent.event.hash() == self.current_root
            {
                return self.current_root
            }

            node_a = node_a_parent.clone();
            node_b = node_b_parent.clone();
        }

        node_a.event.hash().clone()
    }

    fn diff_depth(&self, node_a: EventNodePtr, node_b: EventNodePtr) -> u32 {
        let ancestor = self.find_ancestor(node_a.clone(), node_b.clone());
        let node_a_depth = self.find_depth(node_a, ancestor);
        let node_b_depth = self.find_depth(node_b, ancestor);
        (node_b_depth + 1) - node_a_depth
    }

    async fn debug(&self) {
        for (event_id, event_node) in &self.event_map {
            let depth = self.find_depth(event_node.clone(), self.current_root);
            println!("{}: {:?} [depth={}]", hex::encode(&event_id), event_node.event, depth);
        }

        println!("root: {}", hex::encode(&self.get_root().event.hash()));
        println!("head: {}", hex::encode(&self.find_head().await.event.hash()));
    }
}

pub const CONFIG_FILE: &str = "ircd_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../ircd_config.toml");

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "ircd")]
pub struct Args {
    #[structopt(long)]
    pub config: Option<String>,

    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

fn get_current_time() -> u64 {
    let start = std::time::SystemTime::now();
    start
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
        .try_into()
        .unwrap()
}

fn create_message(previous_event_hash: EventId, nick: &str, msg: &str, timestamp: u64) -> Event {
    Event {
        previous_event_hash,
        action: EventAction::PrivMsg(PrivMsgEvent { nick: nick.to_string(), msg: msg.to_string() }),
        timestamp,
    }
}

struct View {
    seen: HashSet<EventId>,
}

impl View {
    pub fn new() -> Self {
        Self { seen: HashSet::new() }
    }

    fn process(_model: &Model) {
        // This does 2 passes:
        // 1. Walk down all chains and get unseen events
        // 2. Order those events according to timestamp
        // Then the events are replayed to the IRC client
    }
}

async_daemonize!(realmain);
async fn realmain(_settings: Args, _executor: Arc<Executor<'_>>) -> Result<()> {
    let mut model = Model::new();
    let root_id = model.get_root().event.hash();

    let timestamp = get_current_time() + 1;

    let node1 = create_message(root_id, "alice", "alice message", timestamp);
    model.add(node1).await;
    let node2 = create_message(root_id, "bob", "bob message", timestamp);
    let node2_id = node2.hash();
    model.add(node2).await;

    let node3 = create_message(root_id, "charlie", "charlie message", timestamp);
    let node3_id = node3.hash();
    model.add(node3).await;

    let node4 = create_message(node2_id, "delta", "delta message", timestamp);
    let node4_id = node4.hash();
    model.add(node4).await;

    assert_eq!(model.find_head().await.event.hash(), node4_id);

    // Now lets extend another chain
    let node5 = create_message(node3_id, "epsilon", "epsilon message", timestamp);
    let node5_id = node5.hash();
    model.add(node5).await;
    let node6 = create_message(node5_id, "phi", "phi message", timestamp);
    let node6_id = node6.hash();
    model.add(node6).await;

    assert_eq!(model.find_head().await.event.hash(), node6_id);

    model.debug().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn test_update_root() {
        let mut model = Model::new();
        let root_id = model.get_root().event.hash();

        // event_node 1
        // Fill this node with 5 events
        let mut id1 = root_id;
        for x in 0..5 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node).await;
        }

        // event_node 2
        // Fill this node with 14 events
        let mut id2 = root_id;
        for x in 0..14 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node).await;
        }

        // Fill id2 node with 8 events
        let mut id3 = id2;
        for x in 14..22 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id3, &format!("chain 2 msg {}", x), "message", timestamp);
            id3 = node.hash();
            model.add(node).await;
        }

        // Fill id2 node with 9 events
        let mut id4 = id2;
        for x in 14..23 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id4, &format!("chain 2 msg {}", x), "message", timestamp);
            id4 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_height(model.get_root(), id2).await.unwrap(), 0);
        assert_eq!(model.find_height(model.get_root(), id3).await.unwrap(), 8);
        assert_eq!(model.find_height(model.get_root(), id4).await.unwrap(), 9);
        assert_eq!(model.current_root, id2);
    }

    #[async_std::test]
    async fn test_find_height() {
        let mut model = Model::new();
        let root_id = model.get_root().event.hash();

        // event_node 1
        // Fill this node with 8 events
        let mut id1 = root_id;
        for x in 0..8 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node).await;
        }

        // event_node 2
        // Fill this node with 14 events
        let mut id2 = root_id;
        for x in 0..14 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_height(model.get_root(), id1).await.unwrap(), 8);
        assert_eq!(model.find_height(model.get_root(), id2).await.unwrap(), 14);
    }

    #[async_std::test]
    async fn test_prune_forks() {
        let mut model = Model::new();
        let root_id = model.get_root().event.hash();

        // event_node 1
        // Fill this node with 3 events
        let mut event_node_1_ids = vec![];
        let mut id1 = root_id;
        for x in 0..3 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node).await;
            event_node_1_ids.push(id1);
        }

        // event_node 2
        // Start from the root_id and fill the node with 14 events
        // All the events from event_node_1 should get removed from the tree
        let mut id2 = root_id;
        for x in 0..14 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_head().await.event.hash(), id2);

        for id in event_node_1_ids {
            assert!(!model.event_map.contains_key(&id));
        }

        assert_eq!(model.event_map.len(), 15);
    }

    #[async_std::test]
    async fn test_diff_depth() {
        let mut model = Model::new();
        let root_id = model.get_root().event.hash();

        // event_node 1
        // Fill this node with 7 events
        let mut id1 = root_id;
        for x in 0..7 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node).await;
        }

        // event_node 2
        // Start from the root_id and fill the node with 14 events
        // all the events must be added since the depth between id1
        // and the last head is less than 9
        let mut id2 = root_id;
        for x in 0..14 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_head().await.event.hash(), id2);

        // event_node 3
        // This will start as new fork, but no events will be added
        // since the last event's depth is 14
        let mut id3 = root_id;
        for x in 0..3 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id3, &format!("chain 3 msg {}", x), "message", timestamp);
            id3 = node.hash();
            model.add(node).await;

            // ensure events are not added
            assert!(!model.event_map.contains_key(&id3));
        }

        assert_eq!(model.find_head().await.event.hash(), id2);

        // Add more events to the event_node 1
        // At the end this fork must overtake the event_node 2
        for x in 7..14 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_head().await.event.hash(), id1);
    }
}
