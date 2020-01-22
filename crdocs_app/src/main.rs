pub mod server;

extern crate ws;

fn main() {
    let mut signaller = server::Signalling {};

    signaller.start()
}
