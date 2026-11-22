import { basicSetup, EditorView, keymap } from 'codemirror';
import { StreamLanguage, HighlightStyle } from '@codemirror/language';
import { autocompletion, completeFromList } from '@codemirror/autocomplete';
import { tags } from '@lezer/highlight';
import { oneDark } from '@codemirror/theme-one-dark';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import './style.css';

const keywords = new Set(['fn','define','show','print','call','return','if','otherwise','repeat','break','continue','true','false','and','or','not','contract','state','action','view','module','import','as']);
const qorvyntLanguage = StreamLanguage.define({
  name: 'qorvynt',
  startState: () => ({}),
  token(stream) {
    if (stream.eatSpace()) return null;
    if (stream.match('#')) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/"(?:\\.|[^"\\])*"?/)) return 'string';
    if (stream.match(/\d+/)) return 'number';
    if (stream.match(/(?:==|!=|<=|>=|:=|[+*/<>=!-])/)) return 'operator';
    if (stream.match(/[A-Za-z_][A-Za-z0-9_]*/)) {
      const word = stream.current();
      if (word === 'true' || word === 'false') return 'bool';
      if (keywords.has(word)) return 'keyword';
      if (stream.match(/^\s*\(/, false)) return 'function';
      return 'variableName';
    }
    if (stream.match(/[{}()[\],;]/)) return 'punctuation';
    stream.next();
    return null;
  }
});
const qorvyntHighlight = HighlightStyle.define([
  {tag:tags.keyword,color:'#c792ea',fontWeight:'600'},
  {tag:tags.string,color:'#c3e88d'},
  {tag:tags.number,color:'#f78c6c'},
  {tag:tags.bool,color:'#ffcb6b'},
  {tag:tags.comment,color:'#69758c',fontStyle:'italic'},
  {tag:tags.operator,color:'#89ddff'},
  {tag:tags.function(tags.variableName),color:'#82aaff'},
  {tag:tags.variableName,color:'#d6def2'},
  {tag:tags.punctuation,color:'#9ba8c0'}
]);
const completions=['fn','define','show','print','call','return','if','otherwise','repeat','break','continue','contract','state','action','view','true','false','and','or','not'].map(label=>({label,type:'keyword'}));
const status=document.querySelector('#status');
const output=document.querySelector('#output-text');
const fileName=document.querySelector('#file-name');
const tabName=document.querySelector('#tab-name');
let currentPath=null;
let isDirty=false;
let suppressDirty=false;

function setDirty(value) {
  isDirty=value;
  const name=currentPath?.split(/[\\/]/).pop() || 'main.qv';
  tabName.textContent=name+(isDirty?' •':'');
  status.textContent=isDirty?'UNSAVED CHANGES':'ALL CHANGES SAVED';
}
function setCurrentFile(path) {
  currentPath=path;
  const name=path.split(/[\\/]/).pop() || 'main.qv';
  fileName.textContent=name;
  tabName.textContent=name;
  status.textContent='OPENED '+name;
  output.textContent='Loaded '+path;
  setDirty(false);
}
function showError(error) {
  status.textContent='FILE OPERATION FAILED';
  output.textContent=String(error);
}
async function openFile() {
  try {
    if(isDirty && !window.confirm('Discard unsaved changes and open another file?')) return;
    const path=await openDialog({multiple:false,filters:[{name:'QORVYNT Source',extensions:['qv']}]});
    if(!path)return;
    const source=await readTextFile(path);
    suppressDirty=true;
    editor.dispatch({changes:{from:0,to:editor.state.doc.length,insert:source}});
    suppressDirty=false;
    setCurrentFile(path);
  }catch(error){suppressDirty=false;showError(error);}
}
async function saveFile() {
  try {
    const path=currentPath || await saveDialog({defaultPath:'main.qv',filters:[{name:'QORVYNT Source',extensions:['qv']}]});
    if(!path)return;
    await writeTextFile(path,editor.state.doc.toString());
    setCurrentFile(path);
    status.textContent='SAVED '+fileName.textContent;
  }catch(error){showError(error);}
}
const editor = new EditorView({
  doc:'fn main() {\n    define message := "Hello, QORVYNT!"\n    show message\n}',
  parent:document.querySelector('#editor'),
  extensions:[
    basicSetup,oneDark,qorvyntLanguage,qorvyntHighlight,
    autocompletion({override:[completeFromList(completions)]}),
    keymap.of([
      {key:'Mod-s',run:()=>{void saveFile();return true;}},
      {key:'Mod-o',run:()=>{void openFile();return true;}}
    ]),
    EditorView.updateListener.of(update=>{
      if(update.docChanged && !suppressDirty) setDirty(true);
    }),
    EditorView.theme({
      '&':{height:'100%',backgroundColor:'#10131c',fontSize:'13px'},
      '.cm-scroller':{overflow:'auto',fontFamily:'ui-monospace, Consolas, monospace'},
      '.cm-gutters':{backgroundColor:'#10131c',border:'none',color:'#566179'},
      '.cm-activeLineGutter':{backgroundColor:'#192133'},
      '.cm-activeLine':{backgroundColor:'#151d2c'},
      '.cm-cursor':{borderLeftColor:'#aab5ff'},
      '.cm-selectionBackground':{backgroundColor:'#283654 !important'}
    })
  ]
});
document.querySelector('#open-file').addEventListener('click',()=>void openFile());
document.querySelector('#save-file').addEventListener('click',()=>void saveFile());
document.querySelector('#example-file').addEventListener('click',()=>editor.focus());
// Compile/Run remain unavailable until the actual Rust compiler and QVM are exposed over Tauri IPC.
