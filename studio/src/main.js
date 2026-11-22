import './style.css';

const editor = document.querySelector('#editor');
const status = document.querySelector('#status');
const output = document.querySelector('#output-text');

document.querySelector('#example-file').addEventListener('click', () => {
  editor.focus();
  status.textContent = 'EDITING main.qv';
  output.textContent = 'main.qv is open. Native project/file operations are not connected yet.';
});

editor.addEventListener('input', () => {
  status.textContent = 'UNSAVED EDITS · COMPILER NOT CONNECTED';
});

// Compile and Run are deliberately not simulated. They will be wired to the
// real Rust compiler and QVM through Tauri IPC in a later integration step.
window.addEventListener('DOMContentLoaded', () => editor.focus());
