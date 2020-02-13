pub mod server;
use warp::Filter;

use server::*;

use warp::ws::Ws;

use std::path::PathBuf;
use structopt::StructOpt;

use log::*;
#[derive(StructOpt)]
#[structopt(version = "0.0.1", author = "Xavier D.")]
struct Opts {
    #[structopt(short = "p", long = "port", default_value = "3012")]
    port: u16,
    #[structopt(long = "files", default_value = "../frontend/www/dist", parse(from_os_str))]
    files: PathBuf,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let opt = Opts::from_args();

    log::info!("starting up singalling server on port {}", opt.port);

    let state = new_state();

    let users = warp::any().map(move || state.clone());
    let sig = warp::path("sig")
        .and(warp::ws())
        .and(users)
        .map(|ws: Ws, state| {
            ws.on_upgrade(move |socket| handle_connection(state, socket))
        });

    warp::serve(sig.or(warp::fs::dir(opt.files)).with(warp::log("test")))
        .run(([0, 0, 0, 0], opt.port))
        .await;
}
