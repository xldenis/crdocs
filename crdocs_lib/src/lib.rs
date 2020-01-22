pub mod lseq;
mod utils;

pub mod peer;
#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

use wasm_bindgen::prelude::*;

use {
    futures::stream::StreamExt, wasm_bindgen::UnwrapThrowExt, wasm_bindgen_futures::spawn_local,
    ws_stream_wasm::*,
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
pub fn client() {
    let program = async {
        let (mut ws, mut wsio) =
            WsStream::connect("127.0.0.1:3031", None).await.expect_throw("assume the connection succeeds");

        while let Some(msg) = wsio.next().await {
            match msg {
                WsMessage::Text(t) => {
                }
                _ => {}
            }
        }
    };

    spawn_local(program);
}
