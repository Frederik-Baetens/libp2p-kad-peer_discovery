// Copyright 2018 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Demonstrates how to perform Kademlia queries on the IPFS network.
//!
//! You can pass as parameter a base58 peer ID to search for. If you don't pass any parameter, a
//! peer ID will be generated randomly.

use futures::StreamExt;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{GetClosestPeersError, Kademlia, KademliaConfig, KademliaEvent, QueryResult};
use libp2p::multiaddr::Protocol;
use libp2p::{
    development_transport, identity,
    swarm::{Swarm, SwarmEvent},
    Multiaddr, PeerId,
};
use std::{env::args, error::Error, str::FromStr, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // Create a random key for ourselves.
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    println!("local peer_id: {local_peer_id}");
    
    let boot_addr: Option<Multiaddr> = args().nth(2).map(|a| a.parse().unwrap());

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol
    let transport = development_transport(local_key).await?;

    // Create a swarm to manage peers and events.
    let mut swarm = {
        // Create a Kademlia behaviour.
        let mut cfg = KademliaConfig::default();
        cfg.set_query_timeout(Duration::from_secs(5 * 60));
        let store = MemoryStore::new(local_peer_id);
        let mut behaviour = Kademlia::with_config(local_peer_id, store, cfg);

        if let Some(boot_addr) = boot_addr.clone() {
            // Add the bootnodes to the local routing table. `libp2p-dns` built
            // into the `transport` resolves the `dnsaddr` when Kademlia tries
            // to dial these nodes.
            //let mut bootaddr: Multiaddr = "/ip4/147.75.83.83/tcp/4001".parse().unwrap();
            //let boot_peerid = "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb";
            let boot_peerid = if let Protocol::P2p(boot_peerid) = boot_addr.iter().last().unwrap() {
                PeerId::from_multihash(boot_peerid).unwrap()
            } else {
                panic!("invalid boot peerid");
            };
            
            println!("bootaddr: {boot_addr}");
            
            behaviour.add_address(&boot_peerid, boot_addr.clone());
        }
        Swarm::new(transport, behaviour, local_peer_id)
    };

    // Order Kademlia to search for a peer.
    let to_search: PeerId = identity::Keypair::generate_ed25519().public().into();

    if let Some(port) = args().nth(1) {
        swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{port}").parse()?).unwrap();
        println!("listening on port {port}");
    } else {
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?).unwrap();
        use futures::future;
        use futures::Stream;
        use std::task::Poll;
        use std::pin::Pin;
        future::poll_fn(|cx| {
            if swarm.listeners().peekable().peek().is_some() {
                Poll::Ready(None)
            } else {
                Pin::new(&mut swarm).poll_next(cx)
            }
        }).await;
        for addr in swarm.listeners() {
            dbg!(addr);
        }
    }

    println!("Searching for the closest peers to {:?}", to_search);
    swarm.behaviour_mut().get_closest_peers(to_search);
    if (&boot_addr).is_some() {
        swarm.behaviour_mut().bootstrap().unwrap();
    }

    loop {
        let event = swarm.select_next_some().await;
        match event {
            SwarmEvent::Behaviour(KademliaEvent::OutboundQueryCompleted {
                result: QueryResult::GetClosestPeers(result),
                ..
            }) => 
            {
                match result {
                    Ok(ok) => {
                        if !ok.peers.is_empty() {
                            println!("Query finished with closest peers: {:#?}", ok.peers)
                        } else {
                            // The example is considered failed as there
                            // should always be at least 1 reachable peer.
                            println!("Query finished with no closest peers.")
                        }
                    }
                    Err(GetClosestPeersError::Timeout { peers, .. }) => {
                        if !peers.is_empty() {
                            println!("Query timed out with closest peers: {:#?}", peers)
                        } else {
                            // The example is considered failed as there
                            // should always be at least 1 reachable peer.
                            println!("Query timed out with no closest peers.");
                        }
                    }
                };
                for addr in swarm.listeners() {
                    dbg!(addr);
                }
            },
            SwarmEvent::Behaviour(KademliaEvent::RoutingUpdated { peer, addresses, .. }) => {
                let address = addresses.first().to_owned();
                //this is likely useless since these peers are already in the routing table
                println!("adding peer {peer} with address {address}");
                swarm.behaviour_mut().add_address(&peer, address);
            },
            SwarmEvent::Behaviour(evnt) => {dbg!(evnt);},
            _ => (),
        }
    }
}
