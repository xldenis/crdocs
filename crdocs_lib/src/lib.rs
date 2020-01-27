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

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen]
pub fn greet() {
    alert("Hello, crdsssocs!");
}

#[wasm_bindgen]
pub fn greet3() {
    alert("Hello, crdsssocs!");
}

#[wasm_bindgen]
pub fn new_lseq() -> JsValue {
    JsValue::from_serde(&lseq::LSeq::new(0)).unwrap()
}
#[wasm_bindgen]
pub fn lseq_from_js(val: &JsValue) {
    let _l: lseq::LSeq = val.into_serde().unwrap();
}

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
#[wasm_bindgen]
pub async fn client(site_id: u32) {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let lseq = Rc::new(RefCell::new(LSeq::new(site_id)));
    web_sys::console::log_2(&"%d".into(), &site_id.into());

    let (_, wsio) = WsStream::connect("ws://127.0.0.1:3031", None).await.expect_throw("assume the connection succeeds");

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
