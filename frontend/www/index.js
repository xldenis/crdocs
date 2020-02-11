import * as wasm from "crdocs";

const p = new URLSearchParams(window.location.search);

import * as diff from 'diff';

import * as mde from 'simplemde';


const editor_div = document.getElementById('editor');
const peers_span = document.getElementById('peers');
editor_div.value = "";

let signal_url = new URL(window.location)
signal_url.port = 3012
signal_url.protocol = "ws:"

wasm.create_editor(signal_url.toString()).then(function (editor) {
  let editor_window = new mde({ element: editor_div });

  let prev_value = "";
  editor_window.codemirror.on('change', function (e) {
    const ed = e.target;

    let d = diff.diffChars(prev_value, editor_window.codemirror.getValue());
    let ix = 0;

    for (const p of d) {
      console.log(p);
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


  editor.onchange(function(t) {
    if (t != prev_value) {
      prev_value = t;
      // hacky way to avoid cursor resetting on updates
      // should really move to using replaceRange instead....
      let selections = editor_window.codemirror.listSelections();
      editor_window.codemirror.setValue(t);
      editor_window.codemirror.setSelections(selections);
    }
    peers_span.textContent = editor.num_connected_peers();

  });
});
//wasm.test_webrtc_conn(p.get('id'));

