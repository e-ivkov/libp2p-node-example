use crate::{
    helper_fns::{current_time_millis, gen_random_bytes},
    node::NodeBehavior,
};

use async_std::{io, stream, task};
use futures::{future, prelude::*};
use libp2p::{
    floodsub::{self, FloodsubEvent},
    identity,
    mdns::Mdns,
    ping::Ping,
    swarm::NetworkBehaviourEventProcess,
    tcp::TcpConfig,
    websocket::WsConfig,
    Multiaddr, PeerId, Swarm, Transport,
};
use std::time::SystemTime;
use std::{
    error::Error,
    fs::File,
    io::prelude::*,
    task::{Context, Poll},
    time::Duration,
};
extern crate clap;
use crate::transport::upgrade_dev_transport;
use clap::{value_t, App, Arg};

pub mod helper_fns;
pub mod node;
pub mod transport;

//Message size restrictions are 2048 bytes, as mentioned in issue https://github.com/libp2p/rust-libp2p/issues/991
const TX_BYTES: usize = 1000;
const TX_INTERVAL_SEC: usize = 5;

//Node lives this much seconds, then it saves the stats to a file and exits
const NODE_TTL: f64 = 1000.0;

//window size of requests to store and use for statistics
pub const STATS_WINDOW_SIZE: usize = 100;

pub const TCP_TRANSPORT: &str = "tcp";
pub const WEBSOCKET_TRANSPORT: &str = "websocket";

fn main() -> Result<(), Box<dyn Error>> {
    //env_logger::init();
    let matches = App::new("libp2p-node-example")
        .version("0.1.0")
        .author("Egor Ivkov e.o.ivkov@gmail.com")
        .about("Shows libp2p usage scenario as a blockchain node. It is mainly an example to get performance stats of libp2p.")
        .arg(
            Arg::with_name("tx_bytes")
                .long("tx_bytes")
                .value_name("usize")
                .help("Number of pending tx data bytes to generate and forward between nodes. Upper limit is 2048 bytes.")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("tx_interval_sec")
                .long("tx_interval_sec")
                .value_name("usize")
                .help("Interval between sending pending transactions. Simulates the process of getting transactions from clients.")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("node_ttl")
                .long("node_ttl")
                .value_name("f64")
                .help("Number of seconds before this node exits and saves stats.")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("stats_window_size")
                .long("stats_window_size")
                .value_name("usize")
                .help("Number of requests/responses that the stats struct stores to calculate mean.")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("node_addr")
                .long("node_addr")
                .value_name("Multiaddr")
                .help("Valid address of node to reach to connect to swarm.")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("transport")
                .long("transport")
                .short("t")
                .possible_values(&[TCP_TRANSPORT, WEBSOCKET_TRANSPORT])
                .help("Specific transport to use for libp2p higher level protocols.")
                .takes_value(true)
        )
        .get_matches();

    let tx_bytes = value_t!(matches, "tx_bytes", usize).unwrap_or(TX_BYTES);
    let tx_interval_sec = value_t!(matches, "tx_interval_sec", usize).unwrap_or(TX_INTERVAL_SEC);
    let node_ttl = value_t!(matches, "node_ttl", f64).unwrap_or(NODE_TTL);
    let stats_window_size =
        value_t!(matches, "stats_window_size", usize).unwrap_or(STATS_WINDOW_SIZE);

    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Create a Floodsub topic
    let floodsub_topic = floodsub::Topic::new(node::PENDING_TX_FWD_TOPIC);

    // Create a Swarm to manage peers and events
    let mut swarm = {
        let mut behaviour = NodeBehavior::new(local_peer_id.clone(), stats_window_size)?;
        behaviour.floodsub.subscribe(floodsub_topic.clone());
        match matches.value_of("transport") {
            Some(TCP_TRANSPORT) => Swarm::new(
                upgrade_dev_transport(TcpConfig::new(), local_key.clone())?,
                behaviour,
                local_peer_id,
            ),
            Some(WEBSOCKET_TRANSPORT) => Swarm::new(
                upgrade_dev_transport(WsConfig::new(TcpConfig::new()), local_key.clone())?,
                behaviour,
                local_peer_id,
            ),
            _ => Swarm::new(
                libp2p::build_development_transport(local_key.clone())?,
                behaviour,
                local_peer_id,
            ),
        }
    };

    // Reach out to another node if specified
    if let Some(to_dial) = matches.value_of("node_addr") {
        let addr: Multiaddr = to_dial.parse()?;
        Swarm::dial_addr(&mut swarm, addr)?;
        println!("Dialed {:?}", to_dial)
    }

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Simulate periodic appearance of pending transactions
    let mut pending_tx_stream = stream::interval(Duration::from_secs(tx_interval_sec as u64));

    let mut exit_alert = stream::interval(Duration::from_secs_f64(node_ttl));

    let address = if let Some(WEBSOCKET_TRANSPORT) = matches.value_of("transport") {
        "/ip4/0.0.0.0/tcp/0/ws"
    } else {
        "/ip4/0.0.0.0/tcp/0"
    };

    // Listen on all interfaces and whatever port the OS assigns
    Swarm::listen_on(&mut swarm, address.parse()?)?;

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
                    sent_time_millis: current_time_millis(),
                    data: gen_random_bytes(tx_bytes),
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
        match exit_alert.poll_next_unpin(cx) {
            Poll::Ready(Some(_)) => {
                println!("Saving stats");
                swarm.stats.save_to_file("stats.txt")?;
                println!("Exiting");
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => panic!("Exit stream closed"),
            Poll::Pending => Poll::Pending,
        }
    }))
}
