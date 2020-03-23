use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    mdns::{Mdns, MdnsEvent},
    ping::{Ping, PingEvent, PingSuccess},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour,
    PeerId,
};

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
    ignored_member: bool,
}

impl NodeBehavior {
    pub fn new(peer_id: PeerId) -> Result<NodeBehavior, Box<dyn std::error::Error>> {
        let mdns = Mdns::new()?;
        let ping = Ping::default();
        Ok(NodeBehavior {
            floodsub: Floodsub::new(peer_id),
            mdns,
            ping,
            ignored_member: false,
        })
    }
}

impl NetworkBehaviourEventProcess<PingEvent> for NodeBehavior {
    // Called when `ping` produces an event.
    fn inject_event(&mut self, message: PingEvent) {
        if let Result::Ok(PingSuccess::Ping{rtt})= message.result {
            println!("Ping {:?} {:?}", message.peer, rtt);
        }
    }
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for NodeBehavior {
    // Called when `floodsub` produces an event.
    fn inject_event(&mut self, message: FloodsubEvent) {
        if let FloodsubEvent::Message(message) = message {
            if let [topic] = &message.topics[..]{
                match &String::from(topic.clone())[..] {
                    "chat" => println!("Received: '{:?}' from {:?}", String::from_utf8_lossy(&message.data), message.source),
                    _ => println!("Unsupported topic {:?}", topic)
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
            MdnsEvent::Discovered(list) =>
                for (peer, _) in list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            MdnsEvent::Expired(list) =>
                for (peer, _) in list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
        }
    }
}