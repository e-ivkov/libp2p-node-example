use crate::helper_fns::current_time_millis;
use chashmap::CHashMap;
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    mdns::{Mdns, MdnsEvent},
    ping::{Ping, PingEvent, PingSuccess},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour, PeerId,
};
use std::time::Duration;

pub const BLOCK_SYNC_TOPIC: &str = "block_sync";
pub const PENDING_TX_FWD_TOPIC: &str = "pending_tx";
pub const TIME_SYNC_TOPIC: &str = "time_sync";
pub const BLOCK_VOTE_TOPIC: &str = "block_vote";

use p2p_node_stats::Stats;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct PendingTxMessage {
    pub sent_time_millis: u128,
    pub data: Vec<u8>,
}

// We create a custom network behaviour that combines floodsub and mDNS.
// In the future, we want to improve libp2p to make this easier to do.
// Use the derive to generate delegating NetworkBehaviour impl and require the
// NetworkBehaviourEventProcess implementations below.
#[derive(NetworkBehaviour)]
pub struct NodeBehavior {
    pub floodsub: Floodsub,
    pub mdns: Mdns,
    pub ping: Ping,

    // Struct fields which do not implement NetworkBehaviour need to be ignored
    #[behaviour(ignore)]
    pub stats: Stats,
}

impl NodeBehavior {
    pub fn new(peer_id: PeerId, window_size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let mdns = Mdns::new()?;
        let ping = Ping::default();
        Ok(NodeBehavior {
            floodsub: Floodsub::new(peer_id.clone()),
            mdns,
            ping,
            stats: Stats::new(window_size, peer_id.to_string()),
        })
    }
}

impl NetworkBehaviourEventProcess<PingEvent> for NodeBehavior {
    // Called when `ping` produces an event.
    fn inject_event(&mut self, message: PingEvent) {
        if let Result::Ok(PingSuccess::Ping { rtt }) = message.result {
            let peer_id = message.peer;
            println!("Ping {:?} {:?}", peer_id.clone(), rtt);
            self.stats.add_ping(peer_id.to_string(), rtt)
        }
    }
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for NodeBehavior {
    // Called when `floodsub` produces an event.
    fn inject_event(&mut self, message: FloodsubEvent) {
        if let FloodsubEvent::Message(message) = message {
            if let [topic] = &message.topics[..] {
                match &String::from(topic.clone())[..] {
                    PENDING_TX_FWD_TOPIC => {
                        let message_data: PendingTxMessage =
                            bincode::deserialize(&message.data[..]).unwrap();
                        let elapsed_time_millis =
                            current_time_millis() - message_data.sent_time_millis;
                        println!(
                            "Received pending tx {:?} bytes from {:?} in {:?} ms",
                            message.data.len(),
                            message.source,
                            elapsed_time_millis
                        );

                        let peer_id = message.source;
                        self.stats.add_transmission(
                            peer_id.to_string(),
                            Duration::from_millis(elapsed_time_millis as u64),
                            message.data.len() as u32,
                        );
                    }
                    _ => println!("Unsupported topic {:?}", topic),
                }
            } else {
                println!("Received more than 1 topic. Topics: {:?}", message.topics)
            }
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for NodeBehavior {
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}
