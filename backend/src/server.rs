use std::sync::Arc;
use std::sync::Mutex;

use std::collections::HashMap;

use async_std::task;
use futures::channel::mpsc::*;

use async_tungstenite::{tungstenite::Message};

use async_std::net::{TcpListener, TcpStream, SocketAddr};
use futures::{future, stream::{TryStreamExt, StreamExt}, pin_mut};

pub struct Signalling {
    pub clients: Vec<Client>,
}

#[derive(Clone)]
pub struct Client {}


type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

// Simple broadcaster taken from async-tungstenite
async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = async_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    peer_map.lock().unwrap().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        println!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );
        let peers = peer_map.lock().unwrap();

        // We want to broadcast the message to everyone except ourselves.
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| peer_addr != &&addr)
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(msg.clone()).unwrap();
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
}
// Incredibly basic signalling server that just broadcasts every message to everyone
impl Signalling {
    pub async fn start(&mut self) -> () {
        let server = TcpListener::bind("127.0.0.1:3012").await.unwrap();
        let state = Arc::new(Mutex::new(HashMap::new())); 
        while let Ok((stream, addr)) = server.accept().await {
            task::spawn(handle_connection(state.clone(), stream, addr));
        }
    }
}
