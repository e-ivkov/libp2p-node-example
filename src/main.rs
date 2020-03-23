use crate::node::{NodeBehavior, Stats};
use async_std::{io, task};
use futures::{future, prelude::*};
use libp2p::{
    floodsub::{self, FloodsubEvent},
    identity,
    mdns::Mdns,
    ping::Ping,
    swarm::NetworkBehaviourEventProcess,
    Multiaddr, PeerId, Swarm,
};
use std::{
    error::Error,
    task::{Context, Poll},
};

pub mod node;

fn main() -> Result<(), Box<dyn Error>> {
    //env_logger::init();

    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::build_development_transport(local_key)?;

    // Create a Floodsub topic
    let floodsub_topic = floodsub::Topic::new(node::PENDING_TX_FWD_TOPIC);

    // Create a Swarm to manage peers and events
    let mut swarm = {
        let mut behaviour = NodeBehavior::new(local_peer_id.clone())?;
        behaviour.floodsub.subscribe(floodsub_topic.clone());
        Swarm::new(transport, behaviour, local_peer_id)
    };

    // Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        let addr: Multiaddr = to_dial.parse()?;
        Swarm::dial_addr(&mut swarm, addr)?;
        println!("Dialed {:?}", to_dial)
    }

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Listen on all interfaces and whatever port the OS assigns
    Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Kick it off
    let mut listening = false;
    task::block_on(future::poll_fn(move |cx: &mut Context| {
        loop {
            match stdin.try_poll_next_unpin(cx)? {
                Poll::Ready(Some(line)) => {
                    if line == "stats" {
                        println!("{}", swarm.stats)
                    } else {
                        swarm
                            .floodsub
                            .publish(floodsub_topic.clone(), line.as_bytes())
                    }
                }
                Poll::Ready(None) => panic!("Stdin closed"),
                Poll::Pending => break,
            }
        }
        loop {
            match swarm.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => println!("{:?}", event),
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => {
                    if !listening {
                        for addr in Swarm::listeners(&swarm) {
                            println!("Listening on {:?}", addr);
                            listening = true;
                        }
                    }
                    break;
                }
            }
        }
        Poll::Pending
    }))
}
