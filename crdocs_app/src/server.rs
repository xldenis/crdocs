use ws::*;

pub struct Signalling {}

// Incredibly basic signalling server that just broadcasts every message to everyone
impl Signalling {
    pub fn start(&mut self) -> () {
        listen("0.0.0.0:3031", |out| {
            move |msg| {
                out.broadcast(msg)
            }
        })
        .unwrap()
    }
}
