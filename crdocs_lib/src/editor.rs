
use crate::lseq;
use crate::webrtc::*;

pub struct Editor {
    network: NetworkLayer<WsIo>,
    store: lseq::LSeq,
}

use ws_stream_wasm::*;
use futures::stream::*;
use std::error;

pub async fn conncect_and_get_id(url: String) -> Result<(u32, WsIo), Box<error::Error>>  {
    let (_, mut wsio) = WsStream::connect("ws://127.0.0.1:3012", None).await?;
    
    let init_msg = wsio.next().await.expect("first message");

    match init_msg {
        WsMessage::Text(msg) => {
            use serde::Deserialize;
            #[derive(Deserialize)]
            struct InitMsg { site_id: u32, initial_peer: u32 };
            let init_msg : InitMsg = serde_json::from_str(&msg)?;

            return Ok((init_msg.site_id, wsio))
        }
        _ => panic!("First message should be json")
    }
}


pub struct NetworkLayer<T> {
    signaller: T,
    peers: Vec<SimplePeer>,
}

impl<T> NetworkLayer<T> 
    where T : Stream
{
    pub fn new(signalling_channel: T) -> NetworkLayer<T> {
        NetworkLayer {
            signaller: signalling_channel,
            peers: Vec::new(),
        }
    }
}
