use crate::causal::*;
use crate::lseq::*;
use crate::network::*;

use futures::channel::mpsc::*;
use futures::stream::*;

use js_sys::Function;

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::*;

type Shared<T> = Rc<RefCell<T>>;

#[wasm_bindgen]
pub struct Editor {
    network: Rc<NetworkLayer>,
    store: Shared<LSeq>,
    onchange: Shared<Option<Function>>,
    causal: Shared<CausalityBarrier<Op>>,
    pub site_id: u32,
}

use serde::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
enum EditorOp {
    AntiEntropyResp { vec: Vec<Op> },
    AntiEntropyReq { site_id: u32, vec: std::collections::HashMap<SiteId, VectorEntry> },
    Op {
        #[serde(flatten)]
        op: Op
    },
}

impl CausalOp for Op {
    // type Id = Identifier;

    fn happens_before(&self) -> Option<(SiteId, LogTime)> {
        match self {
            Op::Insert{..} => None,
            Op::Delete{remote: (s, t) ,..} => Some((SiteId::from(*s), LogTime::from(*t))),
        }
    }
    // fn id(&self) -> Self::Id {
    //     match self {
    //         Op::Insert{id,..} => id,
    //         Op::Delete{id,..} => id,

    //     }.clone()
    // }
    fn site(&self) -> SiteId {
        match self {
            Op::Insert{site_id, ..} => SiteId::from(*site_id),
            Op::Delete{site_id, ..} => SiteId::from(*site_id),
        }
        .clone()
    }

    fn clock(&self) -> LogTime {
        match self {
            Op::Insert{clock, ..} => LogTime::from(*clock),
            Op::Delete{clock, ..} => LogTime::from(*clock),
        }
    }
}

fn shared<T>(t: T) -> Shared<T> {
    Rc::new(RefCell::new(t))
}

impl Editor {
    pub async fn new(net: std::rc::Rc<NetworkLayer>, id: u32, mut rx: UnboundedReceiver<NetEvent>) -> Self {
        let store = shared(LSeq::new(id));
        let local_store = store.clone();

        let onchange: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
        let local_change = onchange.clone();

        let barrier : Shared<CausalityBarrier<Op>> = shared(CausalityBarrier::new(SiteId(id)));
        let local_barrier = barrier.clone();
        let local_net = net.clone();

        spawn_local(async move {
            while let Some(msg) = rx.next().await {
                log::info!("{:?}", msg);
                match msg {
                    NetEvent::Connection(remote) => {
                        //We just connected to a new peer! Let's check to see if they have any
                        //events we didn't see.

                        let msg = serde_json::to_string(&EditorOp::AntiEntropyReq { site_id: id, vec: local_barrier.borrow().vvwe()}).unwrap();
                        local_net.unicast(remote, &msg).unwrap();

                    }
                    NetEvent::Msg(msg) => { Self::handle_message(msg, &local_net, local_barrier.clone(), local_store.clone(), &local_change.borrow()).await }

                }

            }
        });

        Editor { network: net, store, onchange, causal: barrier, site_id: id }
    }

    async fn handle_message(msg: JsValue, net: &NetworkLayer, causal: Shared<CausalityBarrier<Op>>, lseq: Shared<LSeq>, notif: &Option<Function>) {
        let op = serde_json::from_str(&msg.as_string().unwrap()).expect("parsing message");
        use EditorOp::*;

        match op {
            AntiEntropyReq { site_id, vec } => {
                // Here are all the operations that we've seen and need to find.
                let to_find = causal.borrow().diff_from(&vec);
                let mut resp = Vec::new();
                // 1. Search the local LSeq.
                for e in lseq.borrow().raw_text() {
                    if let Some(set) = to_find.get(&e.2.into()) {
                        if set.contains(&LogTime::from(e.1)) {
                            resp.push(crate::lseq::Op::Insert { id: e.0.clone(), clock: e.1, site_id: e.2, c: e.3 });
                        }
                    }
                }

                // 2. Search the causal buffer for unapplied removes
                for (_causal_id, del) in causal.borrow().buffer.iter() {
                    resp.push(del.clone());
                }

                net.unicast(site_id, &serde_json::to_string(&AntiEntropyResp { vec: resp }).unwrap()).unwrap();

            }
            AntiEntropyResp {vec} => {
                for op in vec {
                    if let Some(op) = causal.borrow_mut().ingest(op) {
                        lseq.borrow_mut().apply(&op);
                    }
                }
                let t = lseq.borrow().text();
                Self::notify_js(notif, &t);
            }
            Op { op } => {
                if let Some(op) = causal.borrow_mut().ingest(op) {
                    net.broadcast(&serde_json::to_string(&Op {op: op.clone()}).unwrap()).await.unwrap();
                    lseq.borrow_mut().apply(&op);
                    let t = lseq.borrow().text();
                    Self::notify_js(notif, &t);
                }

            }
        }
    }

    fn notify_js(this: &Option<Function>, s: &str) {
        match this {
            Some(x) => {
                x.call1(&JsValue::NULL, &s.into()).unwrap();
            }
            None => {}
        }
    }
}

#[wasm_bindgen]
impl Editor {
    pub fn insert(&mut self, c: char, pos: usize) -> js_sys::Promise {
        let op = self.store.borrow_mut().local_insert(pos, c);
        let caus = self.causal.borrow_mut().expel(op);
        let loc_net = self.network.clone();

        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&EditorOp::Op {op: caus}).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn delete(&mut self, pos: usize) -> js_sys::Promise {
        let op = self.store.borrow_mut().local_delete(pos);
        let caus = self.causal.borrow_mut().expel(op);

        let loc_net = self.network.clone();
        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&EditorOp::Op {op: caus}).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn onchange(&mut self, f: &js_sys::Function) {
        self.onchange.replace(Some(f.clone()));
    }

    pub fn num_connected_peers(&mut self) -> usize {
        self.network.num_connected_peers()
    }
}

