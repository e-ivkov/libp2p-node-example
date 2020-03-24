use crate::{
    helper_fns::gen_random_bytes,
    node::{NodeBehavior, Stats},
};

use async_std::{io, stream, task};
use futures::{future, prelude::*};
use libp2p::{
    floodsub::{self, FloodsubEvent},
    identity,
    mdns::Mdns,
    ping::Ping,
    swarm::NetworkBehaviourEventProcess,
    Multiaddr, PeerId, Swarm,
};
use std::time::SystemTime;
use std::{
    error::Error,
    task::{Context, Poll},
    time::Duration,
};

pub mod helper_fns;
pub mod node;

//Message size restrictions are 1 MiB, though noticed that sometimes smaller messages also do not propagate
const TX_BYTES: usize = 1000;
const TX_INTERVAL_SEC: usize = 5;

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

    // Simulate periodic appearance of pending transactions
    let mut pending_tx_stream = stream::interval(Duration::from_secs(TX_INTERVAL_SEC as u64));

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
                    }
                }
                Poll::Ready(None) => panic!("Stdin closed"),
                Poll::Pending => break,
            }
        }
        match pending_tx_stream.poll_next_unpin(cx) {
            Poll::Ready(Some(_)) => {
                //Simulate pending transactions data
                let tx_message = node::PendingTxMessage {
                    sent_time_millis: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .expect("Failed to get duration since UNIX_EPOCH.")
                        .as_millis(),
                    data: gen_random_bytes(TX_BYTES),
                };
                println!("Forwarding pending tx data");
                swarm.floodsub.publish(
                    floodsub_topic.clone(),
                    bincode::serialize(&tx_message).expect("Failed to serialize message."),
                );
            }
            Poll::Ready(None) => panic!("Interval stream closed"),
            Poll::Pending => (),
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
