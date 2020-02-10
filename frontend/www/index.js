import * as wasm from "crdocs";

const p = new URLSearchParams(window.location.search);

import * as diff from 'diff';

const editor_div = document.getElementById('editor');
const peers_span = document.getElementById('peers');
editor_div.value = "";

let signal_url = new URL(window.location)
signal_url.port = 3012
signal_url.protocol = "ws:"

wasm.create_editor(signal_url.toString()).then(function (editor) {
  let prev_value = "";
  editor_div.oninput = function (e) {
    const ed = e.target;


    let d = diff.diffChars(prev_value, editor_div.value);
    let ix = 0;

    for (const p of d) {
      for (const c of p.value) {
        if (p.added) {
          editor.insert(c, ix);
        } else if (p.removed) {
          editor.delete(ix);
        }
        ix++;
      }
    }
    prev_value = ed.value;
  }


  editor.onchange(function(t) {
    editor_div.value = t;
    prev_value = t;
    peers_span.textContent = editor.num_connected_peers();

  });
});
//wasm.test_webrtc_conn(p.get('id'));

