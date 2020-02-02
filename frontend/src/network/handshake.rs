use super::*;
use futures::channel::mpsc::UnboundedReceiver;
use futures::Sink;

pub struct State<S> {
    pub initiator: bool,
    pub sender: S,
    pub remote_id: u32,
    pub peer_recv: UnboundedReceiver<super::HandshakeProtocol>,
    pub local_chan: UnboundedSender<JsValue>,
}

impl<S> State<S>
where
    S: Sink<super::HandshakeProtocol> + Unpin,
    S::Error: std::fmt::Debug,
{
    pub async fn handle_new_peer(&mut self) -> Result<(SimplePeer, DataChannelStream), js_sys::Error> {
        use HandshakeProtocol::*;

        let (mut peer, peer_events) = SimplePeer::new()?;
        let _dc = peer.create_data_channel("peer-connection", 0);

        debug!("starting handshake with {}", self.remote_id);
        // LETS DO THE WEBRTC DANCE
        if self.initiator {
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
        } else {
            debug!("waiting for offer remote_id={}", self.remote_id);
            // 2. Handle the offer
            if let Offer { off } = self.peer_recv.next().await.expect("handle_new_peer") {
                debug!("got offer remote_id={}", self.remote_id);
                let ans = peer.handle_offer(off).await?;
                // 3. Create an answer
                self.sender.send(Answer { ans }).await.expect("handle_new_peer");
            }
        }

        //let mut sink = self.signal.clone();

        info!("Exchanging ICE candidates remote_id={}", self.remote_id);
        self.exchange_ice_candidates(&mut peer, peer_events).await;

        info!("finished exchanging ICE state={:?} remote_id={}", peer.ice_connection_state(), self.remote_id);

        let (dcs, rx) = DataChannelStream::new(_dc);
        let recv_from_peer = self.local_chan.clone();

        // Forward messages from the peer to single queue
        spawn_local(async move {
            rx.map(Ok).forward(recv_from_peer).await.expect("forward");
        });

        // self.peers.borrow_mut().push((peer, dcs));
        Ok((peer, dcs))
    }

    async fn exchange_ice_candidates(
        &mut self,
        peer: &mut SimplePeer,
        mut peer_events: UnboundedReceiver<web_sys::RtcIceCandidate>,
    ) {
        use wasm_timer::Interval;
        use HandshakeProtocol::*;

        use core::time::Duration;
        let mut i = Interval::new(Duration::from_millis(50));

        while let Some(_) = i.next().await {
            futures::select! {
                msg = peer_events.next() => {
                    if let Some(c) = msg {
                        Self::send_candidate(&mut self.sender, c).await
                    }
                },
                msg = self.peer_recv.next() => if let Some(Ice { ice }) = msg {
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
    async fn send_candidate(sender: &mut S, ice: web_sys::RtcIceCandidate) {
        match js_sys::JSON::stringify(&ice.into()) {
            Err(_) => {
                warn!("couldn't serialize ICE candidate");
            }
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
}
