import * as wasm from "crdocs";

const p = new URLSearchParams(window.location.search);

import * as diff from 'diff';

import * as mde from 'simplemde';


const editor_div = document.getElementById('editor');
const peers_span = document.getElementById('peers');
const debug_div = document.getElementById('debug');

editor_div.value = "";

let signal_url = new URL(window.location)
if (signal_url.protocol == "https:") {
  signal_url.protocol = "wss:"
} else {
  signal_url.protocol = "ws:"
}
signal_url.pathname = "sig"
let editor_window = new mde({ element: editor_div });

wasm.create_editor(signal_url.toString()).then(function (editor) {

  let prev_value = "";
  editor_window.codemirror.on('change', function (e) {
    const ed = e.target;

    let d = diff.diffChars(prev_value, editor_window.codemirror.getValue());
    let ix = 0;

    for (const p of d) {
      for (const c of p.value) {
        if (p.added) {
          editor.insert(c, ix);
          ix++;
        } else if (p.removed) {
          editor.delete(ix);
        } else {
          ix++;
        }

      }
    }
    prev_value = editor_window.codemirror.getValue();
  });

  editor.ondebuginfo(function (t) {
    debug_div.innerHTML += "<p>" + t + "</p>";
    peers_span.textContent = editor.num_connected_peers();
  });
  editor.onchange(function(t) {
    if (t != prev_value) {
      prev_value = t;
      // hacky way to avoid cursor resetting on updates
      // should really move to using replaceRange instead....
      let selections = editor_window.codemirror.listSelections();
      editor_window.codemirror.setValue(t);
      editor_window.codemirror.setSelections(selections);
    }
  });
});
//wasm.test_webrtc_conn(p.get('id'));

