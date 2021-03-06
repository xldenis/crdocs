use std::sync::Arc;
use std::sync::Mutex;
use log::*;

use std::collections::*;

use futures::channel::mpsc::*;

use futures::{future, stream::{TryStreamExt, StreamExt}, pin_mut};

use warp::ws::{Message, WebSocket};
use rand::{thread_rng, seq::IteratorRandom};

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

#[derive(Deserialize, Serialize, Debug)]
enum SignalMsg {
    NewSite { },
    Reconn { site_id: u32 },
    InitMsg { site_id: u32, initial_peer: u32},
}

use SignalMsg::*;

// Simple broadcaster taken from async-tungstenite
pub async fn handle_connection(state: PeerMap, mut ws: WebSocket) {
    let initial_peer = state.lock().unwrap().peers.keys().choose(&mut thread_rng()).map(|i| *i);

    let (tx, rx) = unbounded();
    let peer_id = {
        let txt = ws.next().await.unwrap();
        let msg = serde_json::from_str(txt.unwrap().to_str().unwrap()).unwrap();

        match msg {
            NewSite { } => {
                let mut peers = state.lock().unwrap();
                let peer_id = peers.num_peers;
                peers.peers.insert(peer_id, tx);
                peers.num_peers += 1;
                peer_id
            }
            // Only valid if the signalling server hasn't been restarted since
            // assigning ids. TODO: Change this to use UUIDs instead so that
            // there's no need to worry about re-assigning an id.
            Reconn { site_id } => {
                let mut peers = state.lock().unwrap();
                peers.peers.insert(site_id, tx);
                peers.num_peers = peers.num_peers.max(site_id + 1);
                site_id
            }
            _ => { return }
        }
    };

    log::info!("peer {} connected", peer_id);
    let (mut outgoing, incoming) = ws.split();

    use futures::sink::SinkExt;

    outgoing.send(
        Message::text(serde_json::to_string(&InitMsg {
            site_id: peer_id,
            initial_peer: initial_peer.unwrap_or(peer_id),

        }).unwrap())
    ).await.unwrap();

    let broadcast_incoming = incoming.try_for_each(|msg| unicast_msg(&state, peer_id, msg));

    let receive_from_others = rx.map(|msg| { log::info!("sending {} {:?}", peer_id, msg); msg}).map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    state.lock().unwrap().peers.remove(&peer_id);
}

async fn unicast_msg(state: &Mutex<PeerState>, peer_id: u32, msg: Message) -> Result<(), warp::Error>{
    let peers = &mut state.lock().unwrap().peers;
    if let Ok(msg) = msg.to_str() {
        let msg = serde_json::from_str(msg);

        match msg {
            Ok(SignalMessage { id, msg }) => {
                let msg = Message::text(serde_json::to_string(
                        &SignalMessage { id: peer_id, msg }
                ).unwrap());

                if let Some(chan) = peers.get_mut(&id) {
                    chan.unbounded_send(msg).expect("send failed")
                } else {
                    log::info!("Tried sending message to non-existent peer {}", id);
                }
            }
            Err(_) => {
                log::error!("agggg {:?}", msg);
            }
        }
    }
    Ok(())
}

pub fn new_state () -> PeerMap {
    Arc::new(Mutex::new(PeerState {
        peers: BTreeMap::new(),
        num_peers: 0,
    }))
}

