pub mod causal;
pub mod editor;
pub mod lseq;
mod utils;
pub mod webrtc;

#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

use wasm_bindgen::prelude::*;

use {
    futures::sink::SinkExt, futures::stream::StreamExt, wasm_bindgen::UnwrapThrowExt,
    wasm_bindgen_futures::spawn_local, ws_stream_wasm::*,
};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use core::time::Duration;
use std::cell::*;
use std::rc::*;
use wasm_timer::Interval;

use lseq::*;

macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
}
use std::panic;
// use serde::{Deserialize, Serialize};
use crate::editor::*;

#[wasm_bindgen]
pub async fn test_network() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let (id, init_pr, io) = connect_and_get_id("").await.unwrap();

    web_sys::console::log_2(&"peer_id=%d".into(), &id.into());

    let (mut net, rx) = NetworkLayer::new(io).await;

    web_sys::console::log_1(&"network started".into());

    if id != init_pr {
        web_sys::console::log_1(&"network started".into());
        net.connect_to_peer(init_pr).await;
    }
}

#[wasm_bindgen]
pub async fn test_webrtc_conn(site_id: u32) {
    use webrtc::*;
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let sender: bool = site_id == 1;
    web_sys::console::log_2(&"sender=%d".into(), &sender.into());
    let (_, wsio) = WsStream::connect("ws://127.0.0.1:3012", None).await.expect_throw("assume the connection succeeds");

    wasm_timer::Delay::new(Duration::from_millis(2000)).await.unwrap();

    let (mut sink, mut stream) = wsio.split();

    let (mut peer, mut peer_events) = SimplePeer::new().unwrap();
    let dc = peer.create_data_channel("peer-connection");

    // LETS DO THE WEBRTC DANCE
    if sender {
        // 1. Create an offer
        let off = peer.create_offer().await.unwrap();
        sink.send(WsMessage::Text(off)).await.unwrap();

        // 4. Handle answer
        if let WsMessage::Text(ans) = stream.next().await.unwrap() {
            peer.handle_answer(ans).await.unwrap();
        }
    } else {
        // 2. Handle the offer
        if let WsMessage::Text(off) = stream.next().await.unwrap() {
            let ans = peer.handle_offer(off).await.unwrap();
            // 3. Create an answer
            sink.send(WsMessage::Text(ans)).await.unwrap();
        }
    }

    // 5. Exchange ICE candidates!

    use js_sys::JSON;
    use std::sync::{Mutex, RwLock};
    let peer = Rc::new(Mutex::new(peer));

    spawn_local(async move {
        while let Some(c) = peer_events.next().await {
            web_sys::console::log_1(&"NEW CANDIDATE SENT".into());

            match JSON::stringify(&c.into()) {
                Err(_) => {}
                Ok(m) => {
                    sink.send(WsMessage::Text(m.as_string().unwrap())).await.unwrap();
                }
            };
        }
    });

    let local_peer = peer.clone();
    spawn_local(async move {
        while let Some(WsMessage::Text(c)) = stream.next().await {
            web_sys::console::log_1(&"NEW CANDIDATE RECEIVED".into());
            let js = JSON::parse(&c).unwrap();
            use wasm_bindgen::JsCast;
            let cand: web_sys::RtcIceCandidateInit = js.clone().dyn_into().unwrap();
            let mut peer = local_peer.lock().unwrap();
            match peer.ice_connection_state() {
                RtcIceConnectionState::Connected => {
                    web_sys::console::log_1(&"omgomgomgomgomgomg".into());
                    break;
                }
                _ => {
                    web_sys::console::log_1(&"ADDING".into());
                    web_sys::console::log_1(&cand.clone().into());
                    peer.add_ice_candidate(cand).await.expect("did not add")
                }
            }
        }
    });

    let mut i = Interval::new(Duration::from_millis(250));
    while let Some(_) = i.next().await {
        web_sys::console::log_1(&peer.lock().unwrap().ice_connection_state().into());
    }
    web_sys::console::log_1(&"omgomgomgomgomgomg".into());
}
#[wasm_bindgen]
pub async fn client(site_id: u32) {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let lseq = Rc::new(RefCell::new(LSeq::new(site_id)));
    web_sys::console::log_2(&"%d".into(), &site_id.into());

    let (_, wsio) = WsStream::connect("ws://127.0.0.1:3012", None).await.expect_throw("assume the connection succeeds");

    wasm_timer::Delay::new(Duration::from_millis(1000)).await.unwrap();

    let (mut sink, mut stream) = wsio.split();

    let recv_loop = enclose! { (lseq) async move {
        while let Some(msg) = stream.next().await {
            match msg {
                WsMessage::Text(t) => {
                    let o : Op = serde_json::from_str(&t).unwrap();
                    lseq.borrow_mut().apply(o);
                }
                _ => {}
            }

        }
    } };

    let send_loop = enclose! { (lseq) async move {
        let mut i = Interval::new(Duration::from_millis(250));
        let mut count : u32 = 0;

        while let Some(_) = i.next().await {
            use rand::Rng;
            let c : char = rand::thread_rng().sample(rand::distributions::Alphanumeric);
            let ix = rand::thread_rng().gen_range(0, lseq.borrow().text().len().max(1));
            let o : Op = lseq.borrow_mut().local_insert(ix, c);
            sink.send(WsMessage::Text(serde_json::to_string(&o).unwrap())).await.unwrap();
            count += 1;

            if count > 100 { break}
        }

    }};

    let print_loop = enclose! { (lseq) async move {
        let mut i = Interval::new(Duration::from_millis(500));
        while let Some(_) = i.next().await {
            web_sys::console::log_2(&"%s".into(), &lseq.borrow().text().into());
        }
    }};
    spawn_local(print_loop);
    spawn_local(recv_loop);
    if site_id == 1 {
        spawn_local(send_loop);
    }
}
