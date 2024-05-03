use std::{thread, sync::{Arc, Mutex}};

mod error;

mod gfx;
use gfx::init_gui;

mod net;
use net::ZeroMQAdapter;

mod scene;
use scene::{
    SceneGraph,
    SceneGraphPtr,
};

mod shader;

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

fn init_zmq(scene_graph: SceneGraphPtr) {
    // detach thread
    let _ = thread::spawn(move || {
        let mut zmq_rpc = ZeroMQAdapter::new(scene_graph);
        zmq_rpc.poll();
    });
}

fn main() {
    let scene_graph = Arc::new(Mutex::new(SceneGraph::new()));
    init_zmq(scene_graph.clone());
    init_gui(scene_graph);
}

