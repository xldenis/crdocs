use gloo_events::*;

use wasm_bindgen_futures::JsFuture;
use web_sys::{
    RtcIceCandidate, RtcIceCandidateInit, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType,
    RtcSessionDescription, RtcSessionDescriptionInit,
};

use wasm_bindgen::JsValue;
pub use web_sys::{RtcDataChannel, RtcDataChannelInit, RtcIceConnectionState, RtcIceGatheringState};

use std::rc::*;
pub struct WebRtc {
    inner: Rc<RtcPeerConnection>,
    on_ice_candidate: Option<EventListener>,
}

// In WebRtc creating a connection is a fairly elaborate process
//
// First things first: To connect two computers they need some established communication channel,
// called for 'signalling'. They will use this channel to exchange the information necessary to
// actually open a p2p connection.
//
// Given a singalling channel here's how two nodes N1 and N2 connect.
//
// 1. They both create an instance of [WebRtc]
// 2. N1 calls creates an offer
// 3. N1 sets the local description to that offer
// 4. N1 sends the offer to N2 via signalling
// 5. N2 sets it's remote description to the offer
// 6. N2 creates an answer
// 7. N2 sends the answer to N1 via signalling
// 8. N1 sets the remove description to the answer
// 9. N1 and N2 begin exchanging ice candidates via signalling channel.
// 10. Once a functional ice candidate is found you have a connection!

pub enum SdpType {
    Offer(String),
    Answer(String),
}

type Err = js_sys::Error;
use wasm_bindgen::JsCast;

impl WebRtc {
    pub fn new() -> Result<Self, wasm_bindgen::JsValue> {
        match RtcPeerConnection::new() {
            Ok(rtc) => Ok(WebRtc { inner: Rc::new(rtc), on_ice_candidate: None }),
            Err(e) => Err(e),
        }
    }

    /// Register a callback to handle the onicecandidate event
    pub fn register_on_ice<F>(&mut self, mut callback: F)
    where
        F: FnMut(&RtcPeerConnectionIceEvent) + 'static,
    {
        let listener = EventListener::new(&self.inner, "icecandidate", move |msg: &web_sys::Event| {
            let event = msg.dyn_ref::<web_sys::RtcPeerConnectionIceEvent>().unwrap();
            callback(event);
        });

        self.on_ice_candidate = Some(listener);
    }

    /// Create an offer and set the local description to match
    pub async fn create_offer(&self) -> Result<String, Err> {
        let create_offer = JsFuture::from(self.inner.create_offer()).await?;
        let offer = RtcSessionDescription::from(create_offer).sdp();
        let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        desc.sdp(&offer);

        JsFuture::from(self.inner.set_local_description(&desc)).await?;

        Ok(offer)
    }

    /// Create an answer to respond to an offer
    pub async fn create_answer(&self) -> Result<String, Err> {
        let answer = JsFuture::from(self.inner.create_answer()).await?;
        let answer = RtcSessionDescription::from(answer).sdp();
        let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        desc.sdp(&answer);

        JsFuture::from(self.inner.set_local_description(&desc)).await?;
        Ok(answer)
    }

    pub async fn set_remote_description(&self, sdp: SdpType) -> Result<(), Err> {
        let desc = match sdp {
            SdpType::Offer(offer) => {
                let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
                desc.sdp(&offer);
                desc
            }
            SdpType::Answer(ans) => {
                let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                desc.sdp(&ans);
                desc
            }
        };

        JsFuture::from(self.inner.set_remote_description(&desc)).await?;

        Ok(())
    }

    pub unsafe fn get_rtc_conn(&self) -> &RtcPeerConnection {
        &self.inner
    }
    pub async fn add_ice_candidate(&mut self, cand: RtcIceCandidateInit) -> Result<(), Err> {
        JsFuture::from(self.inner.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&cand))).await?;
        Ok(())
    }

    pub fn ice_connection_state(&self) -> RtcIceConnectionState {
        self.inner.ice_connection_state()
    }

    pub fn ice_gathering_state(&self) -> RtcIceGatheringState {
        self.inner.ice_gathering_state()
    }

    /// Create a DataChannel. In WASM all channels need to be created before the connection is
    /// opened.
    pub fn create_data_channel(&self, name: &str) -> RtcDataChannel {
        self.inner.create_data_channel(name)
    }
}

use futures::channel::mpsc;
use futures::channel::mpsc::*;
/// Simple abstraction over the previous (abstracted) rtc connection which just turns the ice
/// candidate callbacks into streams of events.
pub struct SimplePeer {
    conn: WebRtc,
    //
    //send: mpsc::UnboundedSender<RtcPeerConnectionIceEvent>,
}

impl SimplePeer {
    pub fn new() -> Result<(Self, UnboundedReceiver<RtcIceCandidate>), wasm_bindgen::JsValue> {
        let mut rtc_conn = WebRtc::new()?;
        let (tx, rx) = mpsc::unbounded();

        rtc_conn.register_on_ice(move |ice_candidate: &RtcPeerConnectionIceEvent| {
            match ice_candidate.candidate() {
                Some(c) => {
                    if let Err(e) = tx.unbounded_send(c) {
                        log::warn!("could not relay ice candidate {:?}", e);
                    }
                }
                None => tx.close_channel(),
            };
        });

        Ok((
            SimplePeer {
                conn: rtc_conn,
                //send: tx,
            },
            rx,
        ))
    }

    pub async fn create_offer(&mut self) -> Result<String, Err> {
        self.conn.create_offer().await
    }
    pub async fn handle_offer(&mut self, off: String) -> Result<String, Err> {
        self.conn.set_remote_description(SdpType::Offer(off)).await?;
        self.conn.create_answer().await
    }

    pub async fn handle_answer(&mut self, ans: String) -> Result<(), Err> {
        self.conn.set_remote_description(SdpType::Answer(ans)).await
    }

    //pub fn ice_candidates(&mut self) -> &mut UnboundedReceiver<RtcIceCandidate> {
    //   &mut self.recv
    //}

    pub async fn add_ice_candidate(&mut self, cand: RtcIceCandidateInit) -> Result<(), Err> {
        self.conn.add_ice_candidate(cand).await
    }

    pub fn ice_connection_state(&self) -> RtcIceConnectionState {
        self.conn.ice_connection_state()
    }

    pub fn ice_gathering_state(&self) -> RtcIceGatheringState {
        self.conn.ice_gathering_state()
    }

    pub fn create_data_channel(&self, name: &str, id: u16) -> RtcDataChannel {
        unsafe {
            self.conn
                .get_rtc_conn()
                .create_data_channel_with_data_channel_dict(name, RtcDataChannelInit::new().id(id).negotiated(true))
        }
    }
}

pub struct DataChannelStream {
    pub chan: RtcDataChannel,
    on_data: EventListener,
    //tx: UnboundedSender<JsValue>,
    //rx : UnboundedReceiver<JsValue>,
}

impl DataChannelStream {
    pub fn new(chan: RtcDataChannel) -> (Self, UnboundedReceiver<JsValue>) {
        let (tx, rx) = unbounded();

        let el = EventListener::new(&chan, "message", move |msg| {
            let event = msg.dyn_ref::<web_sys::MessageEvent>().unwrap();
            tx.unbounded_send(event.data()).expect("send msg");
        });

        (DataChannelStream { chan, on_data: el }, rx)
    }

    pub fn send(&mut self, msg: &str) -> Result<(), Err> {
        Ok(self.chan.send_with_str(msg)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test_configure;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_create_offer() {
        let rtc = WebRtc::new().expect("could not create rtc connection");
        rtc.create_offer().await.expect("create offer");
    }

    #[wasm_bindgen_test]
    async fn test_create_answer() {
        let rtc = WebRtc::new().expect("could not create rtc connection");
        let off = rtc.create_offer().await.unwrap();
        rtc.set_remote_description(SdpType::Offer(off)).await.unwrap();
        rtc.create_answer().await.expect("is good!");
    }

    #[wasm_bindgen_test]
    async fn test_handshake() {
        let rtc1 = WebRtc::new().expect("could not create rtc connection");
        let rtc2 = WebRtc::new().expect("could not create rtc connection");

        let off = rtc1.create_offer().await.expect("create offer");

        rtc2.set_remote_description(SdpType::Offer(off)).await.unwrap();

        let ans = rtc2.create_answer().await.expect("create answer");

        rtc1.set_remote_description(SdpType::Answer(ans)).await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn test_simple_peer() {
        let (mut rtc1, _) = SimplePeer::new().expect("create simplepeer");
        let (mut rtc2, _) = SimplePeer::new().expect("create simplepeer");

        let off = rtc1.create_offer().await.expect("create offer");
        let ans = rtc2.handle_offer(off).await.expect("handle offer");

        let _ = rtc1.handle_answer(ans);
    }
}
