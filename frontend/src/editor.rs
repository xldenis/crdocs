
use crate::lseq::*;
use crate::webrtc::*;

pub struct Editor {
    network: NetworkLayer,
    store: LSeq,
}

use ws_stream_wasm::*;
use futures::stream::*;
use futures::sink::*;
use std::error;

pub async fn connect_and_get_id(url: String) -> Result<(u32, WsIo), Box<error::Error>>  {
    let (_, mut wsio) = WsStream::connect("ws://127.0.0.1:3012", None).await?;

    let init_msg = wsio.next().await.expect("first message");

    match init_msg {
        WsMessage::Text(msg) => {
            #[derive(Deserialize)]
            struct InitMsg { site_id: u32, initial_peer: u32 };
            let init_msg : InitMsg = serde_json::from_str(&msg)?;

            return Ok((init_msg.site_id, wsio))
        }
        _ => panic!("First message should be json")
    }
}

use futures::channel::mpsc::*;
use futures::{future, pin_mut};
use wasm_bindgen_futures::spawn_local;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
enum HandshakeProtocol {
    Start { },
    Offer { off: String },
    Answer { ans: String },
}

#[derive(Deserialize, Serialize)]
struct Sig {
    from: u32,
    msg: HandshakeProtocol,
}

use std::sync::{Mutex, Arc};

pub struct NetworkLayer {
    peers: Arc<Mutex<Vec<SimplePeer>>>,
    sender: UnboundedSender<WsMessage>,
}

impl NetworkLayer
{
    pub async fn new(signalling_channel: WsIo) -> NetworkLayer {
        let (out, inp) = signalling_channel.split();
        let (tx, rx) = unbounded();

        let peers = Arc::new(Mutex::new(Vec::new()));

        let recv = Self::receive_from_network(peers.clone(), inp, tx.clone());
        let trx  = Self::send_to_network(out, rx);

        spawn_local(async {
            pin_mut!(recv, trx);
            future::select(recv, trx).await;
        });

        NetworkLayer {
            peers,
            sender: tx,
        }
    }

    async fn receive_from_network (peers: Arc<Mutex<Vec<SimplePeer>>>, mut input_stream: SplitStream<WsIo>, sender: UnboundedSender<WsMessage>) {
        let mut xxx = HashMap::new();

        while let Some(WsMessage::Text(msg)) = input_stream.next().await {
            use HandshakeProtocol::*;
            match serde_json::from_str(&msg).unwrap() {
                Sig { from, msg: Start {} } => {
                    let (tx, rx) = unbounded();
                    xxx.insert(from, tx);
                    let sss = sender.clone().with(move |msg : HandshakeProtocol | {
                        let json = serde_json::to_string(&Sig { from, msg }).unwrap();
                        future::ok::<WsMessage, SendError>(WsMessage::Text(json))
                    });
                    spawn_local(Self::handle_new_peer(peers.clone(), rx, sss));
                }
                Sig { from, msg } => {
                    xxx.get_mut(&from).unwrap().send(msg).await.unwrap();
                }
            }
        }
    }

    async fn send_to_network (output_stream: SplitSink<WsIo, WsMessage>, input: UnboundedReceiver<WsMessage>) {
        input.map(Ok).forward(output_stream).await.expect("send_to_network");
    }

    async fn handle_new_peer<S> (peers: Arc<Mutex<Vec<SimplePeer>>>, mut peer_recv: UnboundedReceiver<HandshakeProtocol>, mut sender: S)
        where S : Sink<HandshakeProtocol> + Unpin,
              S::Error : std::fmt::Debug,

    {
        use HandshakeProtocol::*;

        let mut peer = SimplePeer::new().unwrap();
        let _dc = peer.create_data_channel("peer-connection");
        // LETS DO THE WEBRTC DANCE
        // 1. Create an offer
        let off = peer.create_offer().await.unwrap();
        sender.send(Offer { off }).await.unwrap();

        // 4. Handle answer
        if let Answer { ans } = peer_recv.next().await.unwrap() {
            peer.handle_answer(ans).await.unwrap();
        }

        peers.lock().unwrap().push(peer);

    }

    async fn connect_to_peer () {
        // 2. Handle the offer
        //if let WsMessage::Text(off) = peer_recv.next().await.unwrap() {
        //   let ans = peer.handle_offer(off).await.unwrap();
        //   // 3. Create an answer
        // sink.send(WsMessage::Text(ans)).await.unwrap();
        // }
    }
}
