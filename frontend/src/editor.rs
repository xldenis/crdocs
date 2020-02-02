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

impl CausalOp for Op {
    type Id = Identifier;

    fn happens_before(&self) -> bool {
        match self {
            Op::Insert(_, _) => false,
            Op::Delete(_) => true,
        }
    }

    fn id(&self) -> Self::Id {
        match self {
            Op::Insert(ix, _) => ix,
            Op::Delete(ix) => ix,
        }
        .clone()
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

        let barrier = shared(CausalityBarrier::new(SiteId(id)));
        let local_barrier = barrier.clone();

        spawn_local(async move {
            while let Some(msg) = rx.next().await {
                let op = serde_json::from_str(&msg.as_string().unwrap()).unwrap();
                let mut lseq = local_store.lock().unwrap();

                if let Some(op) = local_barrier.borrow_mut().ingest(op) {
                    lseq.apply(op);
                }

                Self::notify_js(local_change.clone(), &lseq.text());
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
