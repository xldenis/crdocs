pub mod server;

fn main() {
    let mut signaller = server::Signalling { clients: Vec::new() };

    signaller.start()
}
