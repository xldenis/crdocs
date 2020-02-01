pub mod server;

use async_std::task;

use tide_static_files::StaticFiles;
use futures::pin_mut;

fn main() {
    let mut signaller = server::Signalling { clients: Vec::new() };
    use futures::future;
    let mut app = tide::new();
    app.at("/assets/*path").get(StaticFiles::new("../frontend/www/dist"));

    let x = async {
        println!("Starting static file server");
        app.listen("localhost:5000").await
    };

    let s = async {
        println!("Starting singalling server");
        signaller.start().await
    };

    pin_mut!(x, s);

    task::block_on(future::select(x, s));

}
