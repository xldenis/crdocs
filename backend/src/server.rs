use std::sync::Arc;
use std::sync::Mutex;

use std::collections::*;

use async_std::task;
use futures::channel::mpsc::*;

use async_tungstenite::{tungstenite::Message};

use async_std::net::{TcpListener, TcpStream, SocketAddr};
use futures::{future, stream::{TryStreamExt, StreamExt}, pin_mut};

pub struct Signalling {
    pub clients: Vec<Client>,
}

#[derive(Clone)]
pub struct Client {}

type Tx = UnboundedSender<Message>;

struct PeerState {
    peers: BTreeMap<u32, Tx>,
    num_peers: u32,
}

type PeerMap = Arc<Mutex<PeerState>>;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
struct SignalMessage { id: u32, msg: Value }

// Simple broadcaster taken from async-tungstenite
async fn handle_connection(state: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = match async_tungstenite::accept_async(raw_stream).await {
        Err(e) => {
            println!("Error during the websocket handshake occurred {:?}", e);
            return;
        }
        Ok(e) => e
    };

    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    let peer_id = {
        let mut peers = state.lock().unwrap();
        let peer_id = peers.num_peers;
        peers.peers.insert(peer_id, tx);
        peers.num_peers += 1;
        peer_id
    };

    let (mut outgoing, incoming) = ws_stream.split();

    #[derive(Serialize)]
    struct InitM { site_id: u32, initial_peer: u32 };

    use futures::sink::SinkExt;

    let min_peer = *state.lock().unwrap().peers.keys().nth(0).unwrap_or(&0);
    outgoing.send(Message::Text(serde_json::to_string(&InitM { site_id: peer_id, initial_peer: min_peer }).unwrap())).await.unwrap();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        println!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );

        let peers = &mut state.lock().unwrap().peers;

        match msg.clone() {
            Message::Text(txt) => {
                match serde_json::from_str(&txt) {
                    Ok(SignalMessage { id, msg }) => {
                        let msg = Message::Text(serde_json::to_string(
                                &SignalMessage { id: peer_id, msg }
                        ).unwrap());

                        match peers.get_mut(&id) {
                            Some(chan) => {
                                chan.unbounded_send(msg).expect("send failed")
                            }
                            None => {
                                println!("Tried sending message to non-existent peer {}", id);
                            }
                        }
                    }
                    Err(err) => {
                        println!("{:?}", err);
                    }
                }
            }
            _ => {
                println!("Unsupported binary message!");
            }
        }

        future::ok(())
    });

    let receive_from_others = rx.map(|msg| {println!("sending {}", peer_id); msg}).map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    state.lock().unwrap().peers.remove(&peer_id);
}
// Incredibly basic signalling server that just broadcasts every message to everyone
impl Signalling {
    pub async fn start(&mut self) -> () {
        let server = TcpListener::bind("127.0.0.1:3012").await.unwrap();
        let state = Arc::new(Mutex::new(PeerState {
            peers: BTreeMap::new(),
            num_peers: 0,
        }));
        while let Ok((stream, addr)) = server.accept().await {
            task::spawn(handle_connection(state.clone(), stream, addr));
        }
    }
}
