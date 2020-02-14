use super::*;
use futures::channel::mpsc::UnboundedReceiver;
use futures::Sink;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum HandshakeProtocol {
    Start {},
    Offer { off: String },
    Answer { ans: String },
    Ice { ice: String },
    IceDone {},
}

pub struct State<S> {
    pub initiator: bool,
    pub sender: S,
    pub remote_id: u32,
    pub peer_recv: UnboundedReceiver<HandshakeProtocol>,
}

impl<S> State<S>
where
    S: Sink<super::HandshakeProtocol> + Unpin,
    S::Error: std::fmt::Debug,
{
    pub async fn handle_new_peer(&mut self) -> Result<(SimplePeer, (DataChannelStream, UnboundedReceiver<JsValue>)), js_sys::Error> {
        use HandshakeProtocol::*;

        let (mut peer, peer_events) = SimplePeer::new_with_ice(vec!["stun:stun1.l.google.com:19302"])?;
        let dc = peer.create_data_channel("peer-connection", 0);

        debug!("starting handshake with {}", self.remote_id);
        // LETS DO THE WEBRTC DANCE
        if self.initiator {
            debug!("waiting for offer remote_id={}", self.remote_id);
            // 2. Handle the offer
            if let Offer { off } = self.peer_recv.next().await.expect("handle_new_peer") {
                debug!("got offer remote_id={}", self.remote_id);
                let ans = peer.handle_offer(off).await?;
                // 3. Create an answer
                self.sender.send(Answer { ans }).await.expect("handle_new_peer");
            }
        } else {
            debug!("sending offer remote_id={}", self.remote_id);
            // 1. Create an offer
            let off = peer.create_offer().await?;
            self.sender.send(Offer { off }).await.expect("handle_new_peer");

            debug!("sent offer remote_id={}", self.remote_id);
            // 4. Handle answer
            if let Answer { ans } = self.peer_recv.next().await.expect("handle_new_peer") {
                peer.handle_answer(ans).await?;
            }
            debug!("got answer remote_id={}", self.remote_id);
        }

        // It is absolutely vital to setup the DCS here since we could miss messages sent during ICE candidate exchange
        let (dcs, rx) = DataChannelStream::new(dc);

        info!("Exchanging ICE candidates remote_id={}", self.remote_id);
        self.exchange_ice_candidates(&mut peer, peer_events).await?;

        info!("finished exchanging ICE state={:?} remote_id={}", peer.ice_connection_state(), self.remote_id);

        Ok((peer, (dcs, rx)))
    }

    async fn exchange_ice_candidates(
        &mut self,
        peer: &mut SimplePeer,
        mut peer_events: UnboundedReceiver<web_sys::RtcIceCandidate>,
    )  -> Result<(), js_sys::Error> {
        use wasm_timer::Interval;
        use HandshakeProtocol::*;

        use core::time::Duration;
        let mut i = Interval::new(Duration::from_millis(50));

        while let Some(_) = i.next().await {
            futures::select! {
                msg = self.peer_recv.next() => {
                    match msg {
                        Some(Ice { ice}) => {
                            warn!("received ice candidate candidate={:?}", ice);
                            Self::add_candidate(&ice, peer).await;
                        }
                        _ => {
                            warn!("finished recieving ice candidates");
                            self.peer_recv.close()
                        }
                    }
                },
                msg = peer_events.next() => {
                    debug!("sending ice candidate remote_id={:?} state={:?}", self.remote_id, peer.ice_connection_state());
                    if let Err(_) = Self::send_candidate(&mut self.sender, msg).await {
                        warn!("failed to send ICE candidate");
                    };
                },
                default => {
                    if peer.ice_connection_state() == RtcIceConnectionState::Connected {
                        // Let our peer know that we're ending the exchange
                        if let Err(_) = Self::send_candidate(&mut self.sender, None).await {
                            warn!("failed to send ICE candidate");
                        };

                        break
                    }
                }
                complete => { break },
            };
        }
        Ok(())
    }

    // Send local candidates to a remote peer
    async fn send_candidate(sender: &mut S, ice: Option<web_sys::RtcIceCandidate>) -> Result<(), S::Error> {
        match ice {
            None => {sender.send(HandshakeProtocol::IceDone {}).await?},
            Some(ice) => {
                let ice_string =  js_sys::JSON::stringify(&ice.into()).unwrap();
                sender.send(HandshakeProtocol::Ice { ice: ice_string.into() }).await?;
            }
        };
        Ok(())

    }

    // Add a remote ICE candidate to a local peer connection
    async fn add_candidate(cand: &str, peer: &mut SimplePeer) {
        let js = js_sys::JSON::parse(&cand).unwrap();
        use wasm_bindgen::JsCast;
        let cand: web_sys::RtcIceCandidateInit = js.clone().dyn_into().unwrap();
        peer.add_ice_candidate(cand).await.expect("did not add")
    }
}
