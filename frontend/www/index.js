import * as wasm from "crdocs";

const p = new URLSearchParams(window.location.search);

import * as diff from 'diff';

const editor = document.getElementById('editor');
editor.value = "";

let prev_value = "";
editor.oninput = function (e) { 
  const ed = e.target;


  let d = diff.diffChars(prev_value, editor.value);
  console.log(d);
  prev_value = ed.value;
}

wasm.test_network();
//wasm.test_webrtc_conn(p.get('id'));

