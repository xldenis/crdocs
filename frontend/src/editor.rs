use crate::causal::*;
use crate::lseq::{ident::*, *};
use crate::network::*;

use futures::channel::mpsc::*;
use futures::stream::*;

use js_sys::Function;

// use log::*;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::*;

type Shared<T> = Rc<RefCell<T>>;

#[wasm_bindgen]
pub struct Editor {
    network: Rc<NetworkLayer>,
    store: Rc<Mutex<LSeq>>,
    onchange: Shared<Option<Function>>,
    causal: Shared<CausalityBarrier<Op>>,
    pub site_id: u32,
}

use serde::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
enum EditorOp {
    AntiEntropyResp { vec: Vec<Op> },
    AntiEntropyReq { vec: std::collections::HashMap<SiteId, VectorEntry> },
    Op {
        #[serde(flatten)]
        op: CausalMessage<Op>
    },
}

impl CausalOp for Op {
    fn happens_before(&self) -> bool {
        match self {
            Op::Insert{..} => false,
            Op::Delete{..} => true,
        }
    }

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
    pub async fn new(net: std::rc::Rc<NetworkLayer>, id: u32, mut rx: UnboundedReceiver<JsValue>) -> Self {
        let store = Rc::new(Mutex::new(LSeq::new(id)));
        let local_store = store.clone();

        let onchange: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
        let local_change = onchange.clone();

        let barrier : Shared<CausalityBarrier<Op>> = shared(CausalityBarrier::new(SiteId(id)));
        let local_barrier = barrier.clone();
        let local_net = net.clone();

        spawn_local(async move {
            while let Some(msg) = rx.next().await {
                let op = serde_json::from_str(&msg.as_string().unwrap()).unwrap();
                let mut lseq = local_store.lock().unwrap();
                use EditorOp::*;

                match op {
                    AntiEntropyReq { vec } => {
                        // Here are all the operations that we've seen and need to find.
                        let to_find = local_barrier.borrow().diff_from(&vec);
                        let mut resp = Vec::new();
                        // 1. Search the local LSeq.
                        for e in lseq.raw_text() {
                            if let Some(set) = to_find.get(&e.2.into()) {
                                if set.contains(&LogTime::from(e.1)) {
                                    resp.push(crate::lseq::Op::Insert { id: e.0.clone(), clock: e.1, site_id: e.2, c: e.3 });
                                }
                            }
                        }

                        // 2. Search the causal buffer for unapplied removes
                        for (_causal_id, del) in local_barrier.borrow().buffer.iter() {
                            resp.push(del.clone());
                        }

                        local_net.broadcast(&serde_json::to_string(&AntiEntropyResp { vec: resp }).unwrap()).await.unwrap();

                    }
                    AntiEntropyResp {vec} => {
                        log::info!("{:?}", vec);
                    }
                    Op { op } => {
                        if let Some(op) = local_barrier.borrow_mut().ingest(op) {
                            lseq.apply(&op);
                        }

                        Self::notify_js(local_change.clone(), &lseq.text());
                    }
                }

            }
        });

        Editor { network: net, store, onchange, causal: barrier, site_id: id }
    }

    fn notify_js(this: Shared<Option<Function>>, s: &str) {
        match &*this.borrow() {
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
        let op = self.store.lock().unwrap().local_insert(pos, c);
        let caus = self.causal.borrow_mut().expel(op);
        let loc_net = self.network.clone();
        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&caus).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn delete(&mut self, pos: usize) -> js_sys::Promise {
        let op = self.store.lock().unwrap().local_delete(pos);
        let caus = self.causal.borrow_mut().expel(op);

        let loc_net = self.network.clone();
        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&caus).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn onchange(&mut self, f: &js_sys::Function) {
        self.onchange.replace(Some(f.clone()));
    }
}

