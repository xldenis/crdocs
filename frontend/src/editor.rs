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
use serde::*;

type Shared<T> = Rc<RefCell<T>>;

#[derive(Debug)]
enum EditorError {
    Js(js_sys::Error),
    Json(serde_json::Error),
}

impl From<js_sys::Error> for EditorError {
    fn from(err: js_sys::Error) -> Self {
        EditorError::Js(err)
    }
}

impl From<serde_json::Error> for EditorError {
    fn from(err: serde_json::Error) -> Self {
        EditorError::Json(err)
    }
}

type Result<T> = core::result::Result<T, EditorError>;

#[wasm_bindgen]
pub struct WrappedEditor(Rc<Editor>);

pub struct Editor {
    network: Rc<NetworkLayer>,
    store: Shared<LSeq>,
    onchange: Shared<Option<Function>>,
    ondebuginfo: Shared<Option<Function>>,
    causal: Shared<CausalityBarrier<Op>>,
    pub site_id: u32,
}


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
    fn happens_before(&self) -> Option<(SiteId, LogTime)> {
        match self {
            Op::Insert{..} => None,
            Op::Delete{remote: (s, t) ,..} => Some((SiteId::from(*s), LogTime::from(*t))),
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

impl From<Rc<Editor>> for WrappedEditor {
    fn from(edit: Rc<Editor>) -> Self {
        WrappedEditor(edit)
    }
}

impl Editor {
    pub async fn new(net: Rc<NetworkLayer>, id: u32, rx: UnboundedReceiver<NetEvent>) -> Rc<Self> {
        let store = LSeq::new(id);
        let barrier = CausalityBarrier::new(SiteId(id));
        Self::with_args(net, id, rx, store, barrier).await
    }

    pub async fn with_args(
        net: Rc<NetworkLayer>,
        id: u32,
        rx: UnboundedReceiver<NetEvent>,
        store: LSeq,
        barrier: CausalityBarrier<Op>,
    ) -> Rc<Self> {
        let onchange = shared(None);
        let ondebuginfo = shared(None);

        let editor = Rc::new(Editor { network: net, store: shared(store), onchange, ondebuginfo, causal: shared(barrier), site_id: id });
        let ret = editor.clone();
        spawn_local(async move { editor.message_loop(rx).await });
        ret
    }

    async fn message_loop(&self, mut rx: UnboundedReceiver<NetEvent>) {
        while let Some(msg) = rx.next().await {
            log::info!("{:?}", msg);
            match self.handle_netevent(msg).await {
                Err(EditorError::Js(e)) => web_sys::console::error_1(&e.into()),
                Err(EditorError::Json(e)) => log::error!("{:?}", e),
                _ => {},
            }
        }
    }

    // Handle a network event
    async fn handle_netevent(&self, msg: NetEvent) -> Result<()> {
        match msg {
            NetEvent::Connected(remote) => {
                self.notify_debug(&format!("{:?}", msg));

                //We just connected to a new peer! Let's check to see if they have any
                //events we didn't see.
                let msg = serde_json::to_string(&EditorOp::AntiEntropyReq {
                    site_id: self.site_id,
                    vec: self.causal.borrow().vvwe()
                })?;

                self.network.unicast(remote, &msg)?;

            }
            NetEvent::Msg(msg) => { self.handle_message(msg).await? }

            _ => {
                self.notify_debug(&format!("{:?}", msg))
            }
        }
        Ok(())
    }

    // Handle an actual message from the network
    async fn handle_message(&self, msg: JsValue) -> Result<()> {
        let op = serde_json::from_str(&msg.as_string().unwrap())?;
        use EditorOp::*;

        match op {
            AntiEntropyReq { site_id, vec } => {
                // Here are all the operations that we've seen and need to find.
                let to_find = self.causal.borrow().diff_from(&vec);
                let mut resp = Vec::new();
                // 1. Search the local LSeq.
                for e in self.store.borrow().raw_text() {
                    if let Some(set) = to_find.get(&e.2.into()) {
                        if set.contains(&LogTime::from(e.1)) {
                            resp.push(crate::lseq::Op::Insert { id: e.0.clone(), clock: e.1, site_id: e.2, c: e.3 });
                        }
                    }
                }

                // 2. Search the causal buffer for unapplied removes
                for (_causal_id, del) in self.causal.borrow().buffer.iter() {
                    resp.push(del.clone());
                }

                self.network.unicast(site_id, &serde_json::to_string(&AntiEntropyResp { vec: resp })?)?;
            }
            AntiEntropyResp {vec} => {
                for op in vec {
                    if let Some(op) = self.causal.borrow_mut().ingest(op) {
                        self.store.borrow_mut().apply(&op);
                    }
                }
                self.notify_change();
            }
            Op { op } => {
                if let Some(op) = self.causal.borrow_mut().ingest(op) {
                    self.store.borrow_mut().apply(&op);
                    self.network.broadcast(&serde_json::to_string(&Op {op: op})?).await?;
                    self.notify_change();
                }

            }
        }
        Ok(())
    }

    fn notify_debug(&self, s: &str) {
       if let Some(x) = &*self.ondebuginfo.borrow() {
            x.call1(&JsValue::NULL, &s.into()).unwrap();
        }
    }

    fn notify_change(&self) {
       if let Some(x) = &*self.onchange.borrow() {
            let t = self.store.borrow().text();
            x.call1(&JsValue::NULL, &t.into()).unwrap();
        }
    }
}

#[wasm_bindgen]
impl WrappedEditor {
    pub fn insert(&mut self, c: char, pos: usize) -> js_sys::Promise {
        let op = self.0.store.borrow_mut().local_insert(pos, c);
        let caus = self.0.causal.borrow_mut().expel(op);
        let loc_net = self.0.network.clone();

        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&EditorOp::Op {op: caus}).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn delete(&mut self, pos: usize) -> js_sys::Promise {
        let op = self.0.store.borrow_mut().local_delete(pos);
        let caus = self.0.causal.borrow_mut().expel(op);

        let loc_net = self.0.network.clone();
        future_to_promise(async move {
            loc_net.broadcast(&serde_json::to_string(&EditorOp::Op {op: caus}).unwrap()).await?;
            Ok(true.into())
        })
    }

    pub fn onchange(&mut self, f: &js_sys::Function) {
        self.0.onchange.replace(Some(f.clone()));
    }

    pub fn ondebuginfo(&mut self, f: &js_sys::Function) {
        self.0.ondebuginfo.replace(Some(f.clone()));
    }

    pub fn num_connected_peers(&mut self) -> usize {
        self.0.network.num_connected_peers()
    }

    pub fn refresh(&mut self) {
        self.0.notify_change();
    }
    pub fn save(&mut self) -> std::result::Result<(), JsValue> {
        let local_storage = web_sys::window().unwrap().local_storage()?.unwrap();

        let barrier_data = serde_json::to_string(&*self.0.causal.borrow()).unwrap();
        let store_data = serde_json::to_string(&*self.0.store.borrow()).unwrap();

        local_storage.set_item("crdocs-barrier", &barrier_data)?;
        local_storage.set_item("crdocs-store", &store_data)?;
        local_storage.set_item("crdocs-id", &format!("{}", self.0.site_id))?;
        Ok(())
    }
}

