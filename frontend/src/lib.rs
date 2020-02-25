#![recursion_limit="256"]
pub mod causal;
pub mod editor;
pub mod lseq;
pub mod network;
pub mod webrtc;

use wasm_bindgen::prelude::*;

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

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
pub async fn create_editor(url: String) -> WrappedEditor {
    console_log::init_with_level(log::Level::Info).unwrap();
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let (id, init_pr, io) = connect_and_get_id(&url).await.unwrap();

    web_sys::console::log_2(&"peer_id=%d".into(), &id.into());

    let (net, rx) = NetworkLayer::new(io).await;

    log::info!("network started");

    if id != init_pr {
        log::info!("connecting to initial peer={}", init_pr);

        net.connect_to_peer(init_pr).await;
    }

    WrappedEditor::new(net, id, rx).await

}
