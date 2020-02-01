use crate::lseq::*;
use crate::webrtc::*;

pub struct Editor {
    network: NetworkLayer,
    store: LSeq,
}

use futures::sink::*;
use futures::stream::*;
use std::error;
use wasm_bindgen::JsValue;
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
    Ice { ice: String },
}

#[derive(Deserialize, Serialize)]
struct Sig {
    id: u32,
    msg: HandshakeProtocol,
}

use std::sync::{Arc, Mutex};
use web_sys::console::*;

pub struct NetworkLayer {
    peers: Mutex<Vec<(SimplePeer, DataChannelStream)>>,
    signal: UnboundedSender<WsMessage>,
    local_chan: UnboundedSender<JsValue>,
}

impl NetworkLayer {
    pub async fn new(signalling_channel: WsIo) -> (Arc<NetworkLayer>, UnboundedReceiver<JsValue>) {
        let (out, inp) = signalling_channel.split();
        let (to_sig, from_sig) = unbounded();

        let peers = Mutex::new(Vec::new());
        let (to_local, from_net) = unbounded();

        let net = Arc::new(NetworkLayer { peers, signal: to_sig, local_chan: to_local });

        let recv = net.clone().receive_from_network(inp);
        let trx = Self::send_to_network(out, from_sig);

        spawn_local(async {
            pin_mut!(recv, trx);
            future::select(recv, trx).await;
        });
        (net, from_net)
    }

    // This function acts as a dispatcher to establish connections to new peers
    async fn receive_from_network(self: Arc<Self>, mut input_stream: SplitStream<WsIo>) {
        let mut peer_signalling = HashMap::new();

        while let Some(WsMessage::Text(msg)) = input_stream.next().await {
            use HandshakeProtocol::*;

            log_1(&"RECV".into());

            match serde_json::from_str(&msg).unwrap() {
                Sig { id, msg: Start {} } => {
                    // A peer just asked us to start a peer connection!
                    let (tx, rx) = unbounded();

                    peer_signalling.insert(id, tx);

                    // Ugly wrapper to turn messages into network frames
                    let peer_sender = self.signal.clone().with(move |msg: HandshakeProtocol| {
                        let json = serde_json::to_string(&Sig { id, msg }).unwrap();
                        future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                    });
                    // Spawn an async closure that will handle the handshake with this peer
                    spawn_local(Self::handle_new_peer(self.clone(), true, rx, peer_sender));
                }
                Sig { id, msg } => {
                    match peer_signalling.get_mut(&id) {
                        Some(chan) => chan.send(msg).await.unwrap(),
                        None => {
                            // Create the signalling state machine for this peer
                            let (mut tx, rx) = unbounded();

                            peer_signalling.insert(id, tx.clone());
                            let peer_sender = self.signal.clone().with(move |msg: HandshakeProtocol| {
                                let json = serde_json::to_string(&Sig { id, msg }).unwrap();
                                future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                            });

                            // Set initiator to false, because in this state, we've just received an
                            // offer.
                            spawn_local(Self::handle_new_peer(self.clone(), false, rx, peer_sender));

                            // Forward the message
                            tx.send(msg).await.unwrap()
                        }
                    }
                }
            }
        }
    }

    async fn send_to_network(output_stream: SplitSink<WsIo, WsMessage>, input: UnboundedReceiver<WsMessage>) {
        input.map(Ok).forward(output_stream).await.expect("send_to_network");
    }

    async fn handle_new_peer<S>(
        self: Arc<Self>,
        initiator: bool,
        mut peer_recv: UnboundedReceiver<HandshakeProtocol>,
        mut sender: S,
    ) where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        use HandshakeProtocol::*;

        let (mut peer, mut peer_events) = SimplePeer::new().unwrap();
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

        log_1(&peer.ice_connection_state().into());
        log_1(&"EXCHANGING ICE CANDIDATES".into());

        //let mut sink = self.signal.clone();

        Self::exchange_ice_candidates(&mut sender, &mut peer_events, &mut peer_recv, &mut peer).await;
        log_1(&peer.ice_connection_state().into());

        let (dcs, mut rx) = DataChannelStream::new(_dc);
        let mut recv_from_peer = self.local_chan.clone();

        // Forward messages from the peer to single queue
        spawn_local(async move { rx.map(Ok).forward(recv_from_peer).await.unwrap();});

        self.peers.lock().unwrap().push((peer, dcs));
    }

    async fn exchange_ice_candidates<S>(
        sender: &mut S,
        peer_events: &mut UnboundedReceiver<web_sys::RtcIceCandidate>,
        peer_recv: &mut UnboundedReceiver<HandshakeProtocol>,
        peer: &mut SimplePeer,
    ) where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        use wasm_timer::Interval;
        use HandshakeProtocol::*;

        use core::time::Duration;
        let mut i = Interval::new(Duration::from_millis(50));

        while let Some(_) = i.next().await {
            futures::select! {
                msg = peer_events.next() => {
                    if let Some(c) = msg {
                        Self::send_candidate(c, sender).await
                    }
                },
                msg = peer_recv.next() => if let Some(Ice { ice }) = msg {
                    log_1(&"RECEIVED CANDIDATE".into());
                    Self::add_candidate(&ice, peer).await;
                },
                complete => break,
                default => {
                    if peer.ice_connection_state() != web_sys::RtcIceConnectionState::New  {
                        log_1(&"Connected?".into());
                        break
                    }
                }
            };
        }
    }
    async fn send_candidate<S>(ice: web_sys::RtcIceCandidate, sender: &mut S)
    where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        match js_sys::JSON::stringify(&ice.into()) {
            Err(_) => {}
            Ok(m) => {
                sender.send(HandshakeProtocol::Ice { ice: m.into() }).await.expect("");
            }
        }
    }

    async fn add_candidate(cand: &str, peer: &mut SimplePeer) {
        let js = js_sys::JSON::parse(&cand).unwrap();
        use wasm_bindgen::JsCast;
        let cand: web_sys::RtcIceCandidateInit = js.clone().dyn_into().unwrap();
        peer.add_ice_candidate(cand).await.expect("did not add")
    }
    pub async fn connect_to_peer(&self, id: u32) {
        use HandshakeProtocol::*;
        // from is actually to here TODO: fix name

        let init_msg = serde_json::to_string(&Sig { id: id, msg: Start {} }).unwrap();
        self.signal.clone().send(WsMessage::Text(init_msg)).await.expect("send handshake start");
    }

    pub async fn broadcast(&mut self, msg: &str) -> Result<(), js_sys::Error> {
        for (_, tx) in self.peers.lock().unwrap().iter_mut() {
            tx.chan.send_with_str(msg)?;
        };
        Ok(())

    }
}
