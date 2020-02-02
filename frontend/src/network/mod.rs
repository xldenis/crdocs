use futures::channel::mpsc::*;
use futures::{future, pin_mut};
use futures::{sink::*, stream::*};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error;
use std::rc::Rc;
//use std::sync::Mutex;
use std::cell::RefCell;

use wasm_bindgen::JsValue;
use wasm_bindgen_futures::*;

use ws_stream_wasm::*;

use crate::webrtc::*;
use log::*;

mod handshake;

pub async fn connect_and_get_id(_url: &str) -> Result<(u32, u32, WsIo), Box<dyn error::Error>> {
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum HandshakeProtocol {
    Start {},
    Offer { off: String },
    Answer { ans: String },
    Ice { ice: String },
}

#[derive(Deserialize, Serialize)]
struct Sig {
    id: u32,
    msg: HandshakeProtocol,
}

pub struct NetworkLayer {
    peers: RefCell<Vec<(SimplePeer, DataChannelStream)>>,
    signal: UnboundedSender<WsMessage>,
    local_chan: UnboundedSender<JsValue>,
}

impl NetworkLayer {
    pub async fn new(signalling_channel: WsIo) -> (Rc<NetworkLayer>, UnboundedReceiver<JsValue>) {
        let (out, inp) = signalling_channel.split();
        let (to_sig, from_sig) = unbounded();

        let peers = RefCell::new(Vec::new());
        let (to_local, from_net) = unbounded();

        let net = Rc::new(NetworkLayer { peers, signal: to_sig, local_chan: to_local });

        let recv = net.clone().receive_from_network(inp);
        let trx = Self::send_to_network(out, from_sig);

        spawn_local(async {
            pin_mut!(recv, trx);
            future::select(recv, trx).await;
        });
        (net, from_net)
    }

    fn make_peer_sink(&self, id: u32) -> impl Sink<HandshakeProtocol, Error = SendError> {
        // Ugly wrapper to turn messages into network frames
        let peer_sender = self.signal.clone().with(move |msg: HandshakeProtocol| {
            let json = serde_json::to_string(&Sig { id, msg }).unwrap();
            future::ok::<WsMessage, SendError>(WsMessage::Text(json))
        });
        (peer_sender)
    }

    // This function acts as a dispatcher to establish connections to new peers
    async fn receive_from_network(self: Rc<Self>, mut input_stream: SplitStream<WsIo>) {
        let mut peer_signalling = HashMap::new();

        while let Some(WsMessage::Text(msg)) = input_stream.next().await {
            use HandshakeProtocol::*;

            match serde_json::from_str(&msg).unwrap() {
                Sig { id, msg: Start {} } => {
                    info!("received handshake start from {}", id);
                    // A peer just asked us to start a peer connection!
                    let (tx, rx) = unbounded();
                    let peer_sink = self.make_peer_sink(id);

                    peer_signalling.insert(id, tx);

                    let handshake = handshake::State {
                        initiator: true,
                        sender: peer_sink,
                        remote_id: id,
                        peer_recv: rx,
                        local_chan: self.local_chan.clone(),
                    };

                    // Spawn an async closure that will handle the handshake with this peer
                    spawn_local(Self::handle_new_peer(self.clone(), handshake));
                }
                Sig { id, msg } => {
                    let chan = peer_signalling.entry(id)
                        .or_insert_with(|| {
                            info!("received offer from {}", id);
                            // Create the signalling state machine for this peer
                            let (tx, rx) = unbounded();
                            let peer_sink = self.make_peer_sink(id);

                            // Set initiator to false, because in this state, we've just received an
                            // offer.
                            let handshake = handshake::State {
                                initiator: false,
                                sender: peer_sink,
                                remote_id: id,
                                peer_recv: rx,
                                local_chan: self.local_chan.clone(),
                            };

                            spawn_local(Self::handle_new_peer(self.clone(), handshake));
                            tx.clone()
                        });

                    chan.send(msg).await.expect("1");
                }
            }
        }
    }

    async fn send_to_network(output_stream: SplitSink<WsIo, WsMessage>, input: UnboundedReceiver<WsMessage>) {
        input.map(Ok).forward(output_stream).await.expect("send_to_network");
    }

    async fn handle_new_peer<S>(self: Rc<Self>, mut handshake: handshake::State<S>)
    where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        match handshake.handle_new_peer().await {
            Ok((peer, dcs)) => {
                self.peers.borrow_mut().push((peer, dcs));
            }
            Err(err) => error!("Couldn't establish peer connection {:?}", err),
        }
    }

    // Start a connection to a remote peer
    pub async fn connect_to_peer(&self, id: u32) {
        use HandshakeProtocol::*;
        // from is actually to here TODO: fix name

        let init_msg = serde_json::to_string(&Sig { id, msg: Start {} }).unwrap();
        self.signal.clone().send(WsMessage::Text(init_msg)).await.expect("send handshake start");
    }

    // Send a packet out to the network
    pub async fn broadcast(&self, msg: &str) -> Result<(), js_sys::Error> {
        for (_, tx) in self.peers.borrow_mut().iter_mut() {
            tx.chan.send_with_str(msg)?;
        }
        Ok(())
    }
}