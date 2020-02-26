#![recursion_limit="256"]
pub mod causal;
pub mod editor;
pub mod lseq;
pub mod network;
pub mod webrtc;

use wasm_bindgen::prelude::*;

use web_sys::window;

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
    console_log::init_with_level(log::Level::Debug).unwrap();
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let (id, init_pr, io) = connect_with_id(&url, None).await.unwrap();

    web_sys::console::log_2(&"peer_id=%d".into(), &id.into());

    let (net, rx) = NetworkLayer::new(io).await;

    log::info!("network started");

    if id != init_pr {
        log::info!("connecting to initial peer={}", init_pr);

        net.connect_to_peer(init_pr).await;
    }

    Editor::new(net, id, rx).await.into()

}

#[wasm_bindgen]
pub async fn load_editor(url: String) -> Result<WrappedEditor, JsValue> {
    console_log::init_with_level(log::Level::Debug).unwrap();
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let local_storage = window().unwrap().local_storage()?.unwrap();

    if let Some(id) = local_storage.get_item("crdocs-id")? {
        let id = id.parse::<u32>().unwrap();
        // Assume that if we managed to read an id the rest of the data is there too
        let barrier_data = local_storage.get_item("crdocs-barrier")?.unwrap();
        let store_data = local_storage.get_item("crdocs-store")?.unwrap();

        web_sys::console::log_2(&"peer_id=%d".into(), &id.into());

        let (id, init_pr, io) = connect_with_id(&url, Some(id)).await.unwrap();

        let (net, rx) = NetworkLayer::new(io).await;

        log::info!("network started");

        if id != init_pr {
            log::info!("connecting to initial peer={}", init_pr);

            net.connect_to_peer(init_pr).await;
        }

        let barrier = serde_json::from_str(&barrier_data).unwrap();
        let store = serde_json::from_str(&store_data).unwrap();

        Ok(Editor::with_args(net, id, rx, store, barrier).await.into())
    } else {
        Ok(create_editor(url).await)
    }
}
