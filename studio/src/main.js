import './style.css';
const editor=document.querySelector('#editor');
const status=document.querySelector('#status');
const output=document.querySelector('#output-text');
document.querySelector('#example-file').addEventListener('click',()=>{editor.focus();status.textContent='EDITING main.qv';output.textContent='main.qv is open. Native project operations are not connected yet.';});
editor.addEventListener('input',()=>{status.textContent='UNSAVED EDITS · COMPILER NOT CONNECTED';});
// Do not simulate compilation. Wire this editor to the real Rust compiler/QVM via Tauri IPC first.
