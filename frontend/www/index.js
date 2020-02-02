import * as wasm from "crdocs";

const p = new URLSearchParams(window.location.search);

import * as diff from 'diff';

const editor_div = document.getElementById('editor');
editor_div.value = "";

wasm.test_network().then(function (editor) {
  let prev_value = "";
  editor_div.oninput = function (e) {
    const ed = e.target;


    let d = diff.diffChars(prev_value, editor_div.value);
    let ix = 0;

    console.log(d);
    for (const p of d) {
      for (const c of p.value) {
        if (p.added) {
          console.log({ ix: ix, c: c });
          editor.insert(c, ix);
        } else if (p.removed) {
          console.log({ ix: ix });
          editor.delete(ix);
        }
        ix++;
      }
    }
    prev_value = ed.value;
  }


  editor.onchange(function(t) { editor_div.value = t; prev_value = t; });
});
//wasm.test_webrtc_conn(p.get('id'));

