pub mod causal;
pub mod editor;
pub mod lseq;
pub mod network;
pub mod webrtc;

use wasm_bindgen::prelude::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use std::panic;
// use serde::{Deserialize, Serialize};
use crate::editor::*;
use crate::network::*;

#[wasm_bindgen]
pub async fn test_network() -> Editor {
    console_log::init().unwrap();
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let (id, init_pr, io) = connect_and_get_id("").await.unwrap();

    web_sys::console::log_2(&"peer_id=%d".into(), &id.into());

    let (net, rx) = NetworkLayer::new(io).await;

    log::info!("network started");

    if id != init_pr {
        net.connect_to_peer(init_pr).await;
    }

    Editor::new(net, id, rx).await
}
