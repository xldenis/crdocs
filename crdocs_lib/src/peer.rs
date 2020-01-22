use wasm_bindgen_futures::JsFuture;
use web_sys::{RtcPeerConnection, RtcSdpType, RtcSessionDescription, RtcSessionDescriptionInit};

pub struct Peer {
    // Underlying connection
    peer: RtcPeerConnection,
}

pub enum SdpType {
    Offer(String),
    Answer(String),
}

impl Peer {
    // Create a call offer and set it to the local Session Description
    pub async fn create_offer(&self) -> Result<String, ()> {
        let create_offer = JsFuture::from(self.peer.create_offer()).await.map_err(|_| ())?;
        let offer = RtcSessionDescription::from(create_offer).sdp();
        let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        desc.sdp(&offer);

        JsFuture::from(self.peer.set_local_description(&desc)).await.map_err(|_| ())?;

        Ok(offer)
    }

    pub async fn create_answer(&self) -> Result<String, ()> {
        let answer = JsFuture::from(self.peer.create_answer()).await.map_err(|_| ())?;
        let answer = RtcSessionDescription::from(answer).sdp();
        let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        desc.sdp(&answer);

        JsFuture::from(self.peer.set_local_description(&desc)).await.map_err(|_| ())?;
        Ok(answer)
    }

    pub async fn set_remote_description(&self, sdp: SdpType) -> Result<(), ()> {
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

        JsFuture::from(self.peer.set_remote_description(&desc)).await.map_err(|_| ())?;

        Ok(())
    }
}
