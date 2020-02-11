pub mod server;
use warp::Filter;

use server::*;

use warp::ws::Ws;

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut signaller = server::Signalling { clients: Vec::new() };
    let state = new_state();

    let users = warp::any().map(move || state.clone());
    let sig = warp::path("sig")
        .and(warp::ws())
        .and(users)
        .map(|ws: Ws, state| {
            ws.on_upgrade(move |socket| handle_connection(state, socket))
        });

    warp::serve(sig.or(warp::fs::dir("../frontend/www/dist")).with(warp::log("test")))
        .run(([127, 0, 0, 1], 3012))
        .await;
}
