use std::sync::Arc;
use std::sync::Mutex;
use log::*;

use std::collections::*;

use futures::channel::mpsc::*;

use futures::{future, stream::{TryStreamExt, StreamExt}, pin_mut};

use warp::ws::{Message, WebSocket};

pub struct Signalling {
    pub clients: Vec<Client>,
}

#[derive(Clone)]
pub struct Client {}

type Tx = UnboundedSender<Message>;

pub struct PeerState {
    peers: BTreeMap<u32, Tx>,
    num_peers: u32,
}

type PeerMap = Arc<Mutex<PeerState>>;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
struct SignalMessage { id: u32, msg: Value }

// Simple broadcaster taken from async-tungstenite
pub async fn handle_connection(state: PeerMap, ws: WebSocket) {
    // println!("Incoming TCP connection from: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    let peer_id = {
        let mut peers = state.lock().unwrap();
        let peer_id = peers.num_peers;
        peers.peers.insert(peer_id, tx);
        peers.num_peers += 1;
        peer_id
    };

    let (mut outgoing, incoming) = ws.split();

    #[derive(Serialize)]
    struct InitM { site_id: u32, initial_peer: u32 };

    use futures::sink::SinkExt;

    let min_peer = *state.lock().unwrap().peers.keys().nth(0).unwrap_or(&0);
    outgoing.send(Message::text(serde_json::to_string(&InitM { site_id: peer_id, initial_peer: min_peer }).unwrap())).await.unwrap();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        let peers = &mut state.lock().unwrap().peers;

        match msg.to_str() {
            Ok(txt) => {
                match serde_json::from_str(txt) {
                    Ok(SignalMessage { id, msg }) => {
                        let msg = Message::text(serde_json::to_string(
                                &SignalMessage { id: peer_id, msg }
                        ).unwrap());

                        match peers.get_mut(&id) {
                            Some(chan) => {
                                chan.unbounded_send(msg).expect("send failed")
                            }
                            None => {
                                log::info!("Tried sending message to non-existent peer {}", id);
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("{:?}", err);
                    }
                }
            }
            _ => {
                log::warn!("unsupported msg type")
            }
        }

        future::ok(())
    });

    let receive_from_others = rx.map(|msg| {log::info!("sending {}", peer_id); msg}).map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    state.lock().unwrap().peers.remove(&peer_id);
}
// Incredibly basic signalling server that just broadcasts every message to everyone

pub fn new_state () -> PeerMap {
    Arc::new(Mutex::new(PeerState {
        peers: BTreeMap::new(),
        num_peers: 0,
    }))
}

