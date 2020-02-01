use crate::lseq::*;
use crate::webrtc::*;

pub struct Editor {
    network: NetworkLayer,
    store: LSeq,
}

use futures::sink::*;
use futures::stream::*;
use std::error;
use ws_stream_wasm::*;

pub async fn connect_and_get_id(_url: &str) -> Result<(u32, u32, WsIo), Box<error::Error>> {
    let (_, mut wsio) = WsStream::connect("ws://127.0.0.1:3012", None).await?;

    let init_msg = wsio.next().await.expect("first message");

    match init_msg {
        WsMessage::Text(msg) => {
            #[derive(Deserialize)]
            struct InitMsg {
                site_id: u32,
                initial_peer: u32,
            };
            let init_msg: InitMsg = serde_json::from_str(&msg)?;

            return Ok((init_msg.site_id, init_msg.initial_peer, wsio));
        }
        _ => panic!("First message should be json"),
    }
}

use futures::channel::mpsc::*;
use futures::{future, pin_mut};
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
enum HandshakeProtocol {
    Start {},
    Offer { off: String },
    Answer { ans: String },
}

#[derive(Deserialize, Serialize)]
struct Sig {
    id: u32,
    msg: HandshakeProtocol,
}

use std::sync::{Arc, Mutex};
use web_sys::console::*;

pub struct NetworkLayer {
    peers: Arc<Mutex<Vec<SimplePeer>>>,
    sender: UnboundedSender<WsMessage>,
}

impl NetworkLayer {
    pub async fn new(signalling_channel: WsIo) -> NetworkLayer {
        let (out, inp) = signalling_channel.split();
        let (tx, rx) = unbounded();

        let peers = Arc::new(Mutex::new(Vec::new()));

        let recv = Self::receive_from_network(peers.clone(), inp, tx.clone());
        let trx = Self::send_to_network(out, rx);

        spawn_local(async {
            pin_mut!(recv, trx);
            future::select(recv, trx).await;
        });

        NetworkLayer { peers, sender: tx }
    }

    async fn receive_from_network(
        peers: Arc<Mutex<Vec<SimplePeer>>>,
        mut input_stream: SplitStream<WsIo>,
        sender: UnboundedSender<WsMessage>,
    ) {
        let mut peer_signalling = HashMap::new();

        while let Some(WsMessage::Text(msg)) = input_stream.next().await {
            use HandshakeProtocol::*;
            log_1(&"RECV".into());
            match serde_json::from_str(&msg).unwrap() {
                Sig { id, msg: Start {} } => {
                    let (tx, rx) = unbounded();

                    peer_signalling.insert(id, tx);

                    let peer_sender = sender.clone().with(move |msg: HandshakeProtocol| {
                        let json = serde_json::to_string(&Sig { id, msg }).unwrap();
                        future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                    });
                    spawn_local(Self::handle_new_peer(peers.clone(), true, rx, peer_sender));
                }
                Sig { id, msg } => {
                    log_1(&"GOGOGOGO".into());
                    match peer_signalling.get_mut(&id) {
                    Some(chan) => chan.send(msg).await.unwrap(),
                    None => {
                        let (mut tx, rx) = unbounded();
                        peer_signalling.insert(id, tx.clone());
                        let peer_sender = sender.clone().with(move |msg: HandshakeProtocol| {
                            let json = serde_json::to_string(&Sig { id, msg }).unwrap();
                            future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                        });
                        spawn_local(Self::handle_new_peer(peers.clone(), false, rx, peer_sender));
                        tx.send(msg).await.unwrap()
                    }}
                },
            }
        }
    }

    async fn send_to_network(output_stream: SplitSink<WsIo, WsMessage>, input: UnboundedReceiver<WsMessage>) {
        input.map(Ok).forward(output_stream).await.expect("send_to_network");
    }

    async fn handle_new_peer<S>(
        peers: Arc<Mutex<Vec<SimplePeer>>>,
        initiator: bool,
        mut peer_recv: UnboundedReceiver<HandshakeProtocol>,
        mut sender: S,
    ) where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        use HandshakeProtocol::*;

        let mut peer = SimplePeer::new().unwrap();
        let _dc = peer.create_data_channel("peer-connection");

        log_1(&"starting handshake".into());
        // LETS DO THE WEBRTC DANCE
        if initiator {
            log_1(&"sending offer".into());
            // 1. Create an offer
            let off = peer.create_offer().await.unwrap();
            sender.send(Offer { off }).await.unwrap();

            log_1(&"sent offer".into());
            // 4. Handle answer
            if let Answer { ans } = peer_recv.next().await.unwrap() {
                peer.handle_answer(ans).await.unwrap();
            }
            log_1(&"got answer".into());
        } else {
            log_1(&"waiting for offer".into());
            // 2. Handle the offer
            if let Offer { off } = peer_recv.next().await.unwrap() {
                log_1(&"got offer".into());
                let ans = peer.handle_offer(off).await.unwrap();
                // 3. Create an answer
                sender.send(Answer { ans }).await.unwrap();
            }
            log_1(&"oops".into());
        }

        peers.lock().unwrap().push(peer);
    }

    pub async fn connect_to_peer(&mut self, id: u32) {
        use HandshakeProtocol::*;
        // from is actually to here TODO: fix name

        let init_msg = serde_json::to_string(&Sig { id: id, msg: Start {} }).unwrap();
        self.sender.send(WsMessage::Text(init_msg)).await.expect("send handshake start");
    }

    pub async fn broadcast(&mut self, msg: ()) {
        for conn in self.peers.lock().unwrap().iter() {
            
        }
    }
}
