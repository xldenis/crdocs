use futures::channel::mpsc::*;
use futures::{future, pin_mut};
use futures::{sink::*, stream::*};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error;
use std::rc::Rc;
use std::cell::RefCell;

use wasm_bindgen::JsValue;
use wasm_bindgen_futures::*;

use ws_stream_wasm::*;

use crate::webrtc::*;
use log::*;

mod handshake;

use handshake::HandshakeProtocol;

#[derive(Deserialize, Serialize)]
enum SignalMsg {
    NewSite { },
    Reconn { site_id: u32 },
    InitMsg { site_id: u32, initial_peer: u32},
}

pub async fn connect_with_id(url: &str, id: Option<u32>) -> Result<(u32, u32, WsIo), Box<dyn error::Error>> {
    use SignalMsg::*;
    let (_, mut wsio) = WsStream::connect(url, None).await?;

    let msg = match id {
        Some(i) => { serde_json::to_string(&Reconn{ site_id: i })? }
        None => { serde_json::to_string(&NewSite {})? }
    };

    wsio.send(WsMessage::Text(msg)).await?;

    let init_msg = wsio.next().await.unwrap();

    match init_msg {
        WsMessage::Text(msg) => {
            match serde_json::from_str(&msg)? {
                InitMsg { site_id, initial_peer } => {
                    return Ok((site_id, initial_peer, wsio));
                }
                _ => panic!(""),
            }
        }
        _ => panic!("First message should be json"),
    }
}

#[derive(Deserialize, Serialize)]
struct Sig {
    id: u32,
    msg: HandshakeProtocol,
}

#[derive(Debug)]
pub enum NetEvent {
    Connected(u32),
    Disconnected(u32),
    Failed(u32),
    Connecting(u32),
    Msg(JsValue),
}

use NetEvent::*;

pub struct NetworkLayer {
    // Active Peer Connections
    pub peers: RefCell<HashMap<u32, (SimplePeer, DataChannelStream)>>,
    // Signalling server
    signal: UnboundedSender<WsMessage>,
    // Output channel
    local_chan: UnboundedSender<NetEvent>,
    // On going handshakes with peers
    handshakes: RefCell<HashMap<u32, UnboundedSender<HandshakeProtocol>>>,
}

impl NetworkLayer {
    pub async fn new(signalling_channel: WsIo) -> (Rc<NetworkLayer>, UnboundedReceiver<NetEvent>) {
        let (out, inp) = signalling_channel.split();
        let (signal, from_sig) = unbounded();

        let peers = RefCell::new(HashMap::new());
        let (local_chan, from_net) = unbounded();

        let net = Rc::new(NetworkLayer { peers, signal, local_chan, handshakes: RefCell::new(HashMap::new()) });

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
        while let Some(WsMessage::Text(msg)) = input_stream.next().await {
            use HandshakeProtocol::*;

            let Sig {id, msg} = serde_json::from_str(&msg).unwrap();

            match msg {
                Start {} => {
                    log::debug!("starting handshake because of msg {:?}", msg);
                    self.start_handshake(id, false);
                }
                msg => {
                    if let Some(chan) = self.handshakes.borrow_mut().get_mut(&id) {
                        if let Err(_) = chan.send(msg.clone()).await {
                            error!("Could not handle {:?} from {:?}", msg, id);
                        }
                    }
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
        self.local_chan.clone().send(Connecting(handshake.remote_id)).await.unwrap();

        match handshake.handle_new_peer().await {
            Ok((mut peer, (dcs, rx))) => {
                use RtcIceConnectionState::{Connected, Completed};
                self.handshakes.borrow_mut().remove(&handshake.remote_id);

                // Wait to try all the connection candidates
                peer.wait_for_connection().await;

                if peer.ice_connection_state() != Connected && peer.ice_connection_state() != Completed {
                    warn!("Couldn't establish connection to peer {:?}", peer.ice_connection_state());
                    self.local_chan.clone().send(Failed(handshake.remote_id)).await.unwrap();

                    return;
                }

                let mut recv_from_peer = self.local_chan.clone();

                self.peers.borrow_mut().insert(handshake.remote_id, (peer, dcs));

                // Forward messages from the peer to single queue
                let this = self.clone();
                let id = handshake.remote_id;

                recv_from_peer.send(NetEvent::Connected(handshake.remote_id)).await.unwrap();

                spawn_local(async move {
                    let _ = rx.map(Msg).map(Ok).forward(recv_from_peer.clone()).await;
                    this.peers.borrow_mut().remove(&id);
                    recv_from_peer.send(Disconnected(id)).await.unwrap();
                });

            }
            Err(err) => {
                self.local_chan.clone().send(Failed(handshake.remote_id)).await.unwrap();
                self.handshakes.borrow_mut().remove(&handshake.remote_id);

                error!("Couldn't establish peer connection {:?}", err)
            }
        }
    }

    fn start_handshake(self: &Rc<Self>, id: u32, initiator: bool) {
        // Create the signalling state machine for this peer
        let (tx, rx) = unbounded();
        let peer_sink = self.make_peer_sink(id);

        // Set initiator to false, because in this state, we've just received an
        // offer.
        let handshake = handshake::State {
            initiator: initiator,
            sender: peer_sink,
            remote_id: id,
            peer_recv: rx,
        };
        self.handshakes.borrow_mut().insert(id, tx);
        spawn_local(Self::handle_new_peer(self.clone(), handshake));
    }
    // Start a connection to a remote peer
    pub async fn connect_to_peer(self: &Rc<Self>, id: u32) {
        use HandshakeProtocol::*;
        // from is actually to here TODO: fix name
        self.start_handshake(id, true);

        let init_msg = serde_json::to_string(&Sig { id, msg: Start {} }).unwrap();
        self.signal.clone().send(WsMessage::Text(init_msg)).await.expect("send handshake start");
    }

    // Send a packet out to the network
    pub async fn broadcast(&self, msg: &str) -> Result<(), js_sys::Error> {
        for (_, (_, tx)) in self.peers.borrow_mut().iter_mut() {
            let _ = tx.send(msg);
        }
        Ok(())
    }

    pub fn unicast(&self, ix: u32, msg: &str) -> Result<(), js_sys::Error> {
        match self.peers.borrow_mut().get_mut(&ix) {
            Some((_, stream)) => {
                if let Err(mut e) = stream.send(msg) {
                    e.set_name("UnicastError");
                    Err(e)?
                }
            }
            None => {
                Err(js_sys::Error::new("not connected to peer"))?;
            }
        }
        Ok(())
    }

    pub fn num_connected_peers(&self) -> usize {
        self.peers.borrow().len()
    }
}
