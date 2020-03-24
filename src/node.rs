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

//window size of requests to store and use for statistics
pub const WINDOW_SIZE: usize = 100;

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
    #[allow(dead_code)]
    pub stats: Stats,
}

impl NodeBehavior {
    pub fn new(peer_id: PeerId) -> Result<Self, Box<dyn std::error::Error>> {
        let mdns = Mdns::new()?;
        let ping = Ping::default();
        Ok(NodeBehavior {
            floodsub: Floodsub::new(peer_id),
            mdns,
            ping,
            stats: Stats::new(WINDOW_SIZE),
        })
    }
}

impl NetworkBehaviourEventProcess<PingEvent> for NodeBehavior {
    // Called when `ping` produces an event.
    fn inject_event(&mut self, message: PingEvent) {
        if let Result::Ok(PingSuccess::Ping { rtt }) = message.result {
            let peer_id = message.peer;
            println!("Ping {:?} {:?}", peer_id.clone(), rtt);
            let ping_to_peers = &self.stats.pings_to_peers;
            if !ping_to_peers.contains_key(&peer_id) {
                ping_to_peers.insert_new(peer_id.clone(), Vec::new())
            }
            ping_to_peers
                .get_mut(&peer_id)
                .expect("Failed to get peer entry")
                .push_lossy(rtt, self.stats.window_size)
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
                        let transmission_rates = &self.stats.transmissions_rates;
                        let peer_id = message.source;
                        if !transmission_rates.contains_key(&peer_id) {
                            transmission_rates.insert_new(peer_id.clone(), Vec::new())
                        }
                        transmission_rates
                            .get_mut(&peer_id)
                            .expect("Failed to get peer entry")
                            .push_lossy(
                                //put transmission rate which is elapsed time per byte
                                Duration::from_millis(elapsed_time_millis as u64)
                                    / message.data.len() as u32,
                                self.stats.window_size,
                            )
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

pub struct Stats {
    pings_to_peers: CHashMap<PeerId, Vec<Duration>>,
    transmissions_rates: CHashMap<PeerId, Vec<Duration>>,
    window_size: usize,
}

impl Stats {
    pub fn new(window_size: usize) -> Self {
        Self {
            pings_to_peers: CHashMap::new(),
            transmissions_rates: CHashMap::new(),
            window_size,
        }
    }
}

use std::fmt;

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn avg_duration(durations: Vec<Duration>) -> Option<Duration> {
            if durations.is_empty() {
                None
            } else {
                Some(
                    durations
                        .iter()
                        .fold(Duration::from_secs(0), |acc, x| acc + *x)
                        / durations.len() as u32,
                )
            }
        }

        let ping_by_peer: String = self
            .pings_to_peers
            .clone()
            .into_iter()
            .map(|(peer, durations)| match avg_duration(durations) {
                Some(duration) => format!("{:?} {:?}\n", peer, duration),
                None => format!("No ping data for peer {:?}", peer),
            })
            .collect();

        let transmission_rate_by_peer: String = self
            .transmissions_rates
            .clone()
            .into_iter()
            .map(|(peer, durations)| match avg_duration(durations) {
                Some(duration) => format!("{:?} {:?} per byte\n", peer, duration),
                None => format!("No transmission data for peer {:?}", peer),
            })
            .collect();
        write!(
            f,
            "Average ping for each peer:\n{} Average transmission rate by peer:\n{}",
            ping_by_peer, transmission_rate_by_peer
        )
    }
}

pub trait PushLossy<T> {
    fn push_lossy(&mut self, element: T, window_size: usize);
}

impl<T> PushLossy<T> for Vec<T> {
    fn push_lossy(&mut self, element: T, window_size: usize) {
        if self.len() >= window_size {
            self.remove(0);
        }
        self.push(element);
    }
}
