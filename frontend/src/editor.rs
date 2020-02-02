use crate::lseq::{*, ident::*};
use crate::webrtc::*;

use wasm_bindgen::prelude::*;

use std::cell::RefCell;

use crate::causal::*;

type Shared<T> = Rc<RefCell<T>>;

use js_sys::Function;

#[wasm_bindgen]
pub struct Editor {
    network: Rc<NetworkLayer>,
    store: Rc<Mutex<LSeq>>,
    onchange: Shared<Option<Function>>,
    causal: Shared<CausalityBarrier<Op>>,
    pub site_id: u32,
}

impl CausalOp for Op {
    type Id = Identifier;

    fn happens_before(&self) -> bool {
        match self {
            Op::Insert(_, _) => false,
            Op::Delete(_) => true,
        }
    }

    fn id (&self) -> Self::Id {
        match self {
            Op::Insert(ix, _) => ix,
            Op::Delete(ix) => ix,
        }.clone()
    }
}

fn shared<T>(t: T) -> Shared<T> {
    Rc::new(RefCell::new(t))
}

impl Editor {
    pub async fn new(net: std::rc::Rc<NetworkLayer>, id: u32, mut rx: UnboundedReceiver<JsValue>) -> Self {
        let store = Rc::new(Mutex::new(LSeq::new(id)));
        let local_store = store.clone();

        let onchange : Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
        let local_change = onchange.clone();

        let barrier = shared(CausalityBarrier::new(SiteId(id)));
        let local_barrier = barrier.clone();

        spawn_local(async move {
            while let Some(msg) = rx.next().await {
                let op = serde_json::from_str(&msg.as_string().unwrap()).unwrap();
                let mut lseq  = local_store.lock().unwrap();
                match local_barrier.borrow_mut().ingest(op) {
                    Some(op) => {
                        lseq.apply(op);
                    }
                    None => {}
                }


                Self::notify_js(local_change.clone(), &lseq.text());
            }
        });

        Editor {
            network: net,
            store,
            onchange,
            causal: barrier,
            site_id: id,
        }
    }

    fn notify_js(this: Shared<Option<Function>>, s: &str)  {
        match &*this.borrow() {
            Some(x) => {
                x.call1(&JsValue::NULL, &s.into()).unwrap();
            }
            None => {}
        }
    }
}

use wasm_bindgen_futures::*;

#[wasm_bindgen]
impl Editor {
    pub fn insert(&mut self, c : char, pos: usize) -> js_sys::Promise {
        let op = self.store.lock().unwrap().local_insert(pos, c);
        let caus = self.causal.borrow_mut().expel(op);
        let loc_net = self.network.clone();
        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&caus).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn delete(&mut self, pos: usize) -> js_sys::Promise {
        let op = self.store.lock().unwrap().local_delete(pos);
        let caus = self.causal.borrow_mut().expel(op);

        let loc_net = self.network.clone();
        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&caus).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn onchange(&mut self, f: &js_sys::Function) {
        self.onchange.replace(Some(f.clone()));
    }
}
use futures::sink::*;
use futures::stream::*;
use std::error;
use wasm_bindgen::JsValue;
use ws_stream_wasm::*;

use log::*;

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

use futures::channel::mpsc::*;
use futures::{future, pin_mut};
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
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

use std::sync::Mutex;
use std::rc::Rc;

pub struct NetworkLayer {
    peers: Mutex<Vec<(SimplePeer, DataChannelStream)>>,
    signal: UnboundedSender<WsMessage>,
    local_chan: UnboundedSender<JsValue>,
}

use futures::future::FutureExt;
impl NetworkLayer {
    pub async fn new(signalling_channel: WsIo) -> (Rc<NetworkLayer>, UnboundedReceiver<JsValue>) {
        let (out, inp) = signalling_channel.split();
        let (to_sig, from_sig) = unbounded();

        let peers = Mutex::new(Vec::new());
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

                    peer_signalling.insert(id, tx);

                    // Ugly wrapper to turn messages into network frames
                    let peer_sender = self.signal.clone().with(move |msg: HandshakeProtocol| {
                        let json = serde_json::to_string(&Sig { id, msg }).unwrap();
                        future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                    });
                    // Spawn an async closure that will handle the handshake with this peer
                    spawn_local(Self::handle_new_peer(self.clone(), true, rx, peer_sender, id).map(|r| r.unwrap()));
                }
                Sig { id, msg } => {
                    match peer_signalling.get_mut(&id) {
                        Some(chan) => {
                            match chan.send(msg).await {
                                Ok(_) => {}
                                Err(_) => { warn!("error sending message to handshake"); }
                            }
                        }
                        None => {
                            info!("received offer from {}", id);
                            // Create the signalling state machine for this peer
                            let (mut tx, rx) = unbounded();

                            peer_signalling.insert(id, tx.clone());

                            let peer_sender = self.signal.clone().with(move |msg: HandshakeProtocol| {
                                let json = serde_json::to_string(&Sig { id, msg }).unwrap();
                                future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                            });

                            // Set initiator to false, because in this state, we've just received an
                            // offer.
                            spawn_local(Self::handle_new_peer(self.clone(), false, rx, peer_sender, id).map(|r| r.unwrap()));

                            // Forward the message
                            tx.send(msg).await.expect("1")
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
        self: Rc<Self>,
        initiator: bool,
        mut peer_recv: UnboundedReceiver<HandshakeProtocol>,
        mut sender: S,
        remote_id: u32,
    ) -> Result<(), js_sys::Error> where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        use HandshakeProtocol::*;

        let (mut peer, mut peer_events) = SimplePeer::new()?;
        let _dc = peer.create_data_channel("peer-connection", 0);

        debug!("starting handshake with {}", remote_id);
        // LETS DO THE WEBRTC DANCE
        if initiator {
            debug!("sending offer remote_id={}", remote_id);
            // 1. Create an offer
            let off = peer.create_offer().await?;
            sender.send(Offer { off }).await.expect("handle_new_peer");

            debug!("sent offer remote_id={}", remote_id);
            // 4. Handle answer
            if let Answer { ans } = peer_recv.next().await.expect("handle_new_peer") {
                peer.handle_answer(ans).await?;
            }
            debug!("got answer remote_id={}", remote_id);
        } else {
            debug!("waiting for offer remote_id={}", remote_id);
            // 2. Handle the offer
            if let Offer { off } = peer_recv.next().await.expect("handle_new_peer") {
                debug!("got offer remote_id={}", remote_id);
                let ans = peer.handle_offer(off).await?;
                // 3. Create an answer
                sender.send(Answer { ans }).await.expect("handle_new_peer");
            }
        }

        //let mut sink = self.signal.clone();

        info!("Exchanging ICE candidates remote_id={}", remote_id);
        Self::exchange_ice_candidates(&mut sender, &mut peer_events, &mut peer_recv, &mut peer).await;

        info!("finished exchanging ICE state={:?} remote_id={}", peer.ice_connection_state(), remote_id);

        let (dcs, rx) = DataChannelStream::new(_dc);
        let recv_from_peer = self.local_chan.clone();

        // Forward messages from the peer to single queue
        spawn_local(async move { rx.map(Ok).forward(recv_from_peer).await.expect("forward"); });

        self.peers.lock().unwrap().push((peer, dcs));
        Ok(())
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
                    debug!("received ice candidate candidate={:?}", ice);
                    Self::add_candidate(&ice, peer).await;
                },
                complete => break,
                default => {
                    if peer.ice_connection_state() != web_sys::RtcIceConnectionState::New  {
                        break
                    }
                }
            };
        }
    }

    // Send local candidates to a remote peer
    async fn send_candidate<S>(ice: web_sys::RtcIceCandidate, sender: &mut S)
    where
        S: Sink<HandshakeProtocol> + Unpin,
        S::Error: std::fmt::Debug,
    {
        match js_sys::JSON::stringify(&ice.into()) {
            Err(_) => { warn!("couldn't serialize ICE candidate"); }
            Ok(m) => {
                sender.send(HandshakeProtocol::Ice { ice: m.into() }).await.expect("");
            }
        }
    }

    // Add a remote ICE candidate to a local peer connection
    async fn add_candidate(cand: &str, peer: &mut SimplePeer) {
        let js = js_sys::JSON::parse(&cand).unwrap();
        use wasm_bindgen::JsCast;
        let cand: web_sys::RtcIceCandidateInit = js.clone().dyn_into().unwrap();
        peer.add_ice_candidate(cand).await.expect("did not add")
    }

    // Start a connection to a remote peer
    pub async fn connect_to_peer(&self, id: u32) {
        use HandshakeProtocol::*;
        // from is actually to here TODO: fix name

        let init_msg = serde_json::to_string(&Sig { id: id, msg: Start {} }).unwrap();
        self.signal.clone().send(WsMessage::Text(init_msg)).await.expect("send handshake start");
    }

    // Send a packet out to the network
    pub async fn broadcast(&self, msg: &str) -> Result<(), js_sys::Error> {
        for (_, tx) in self.peers.lock().unwrap().iter_mut() {
            tx.chan.send_with_str(msg)?;
        };
        Ok(())

    }
}

