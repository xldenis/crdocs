import * as wasm from "crdocs";

const p = new URLSearchParams(window.location.search);
wasm.test_webrtc_conn(p.get('id'));
