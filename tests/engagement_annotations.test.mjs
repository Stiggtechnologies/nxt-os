import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto,createHash} from 'node:crypto';

const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
function harness(){
  const editor={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]};
  const inspector={...editor};
  const main={innerHTML:'',querySelector:s=>s==='#editor'?editor:s==='#inspector'?inspector:null,querySelectorAll:()=>[]};
  const escape=v=>String(v).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;').replaceAll("'",'&#39;');
  const sandbox={NXT:{esc:escape,api(){throw Error('No network expected');},modal(){throw Error('Dialog not configured');},toast(){},confirm:async()=>true},
    document:{querySelector:s=>s==='#main'?main:s==='#editor'?editor:s==='#inspector'?inspector:null,querySelectorAll:()=>[]},
    crypto:{randomUUID:()=>webcrypto.randomUUID(),subtle:webcrypto.subtle},TextEncoder,Uint8Array,structuredClone,
    location:{},console};
  // Exercise actual closure functions while excluding the unrelated page bootstrap.
  const boundary=source.indexOf('  const actions={');
  assert.ok(boundary>0);
  vm.runInNewContext(source.slice(0,boundary)+`globalThis.subject={S,addAnnotation,removeAnnotation,renderAnnotations,setDialog:fn=>dialog=fn};})();`,sandbox);
  const api=sandbox.subject;
  api.S.record={id:'eng_synthetic',status:'draft',revision:1,signoffs:[]};
  api.S.draft={workpapers:[{id:'paper1',reference:'A',title:'Synthetic',body:'Café\r\n📎'}],accounts:[],issues:[]};
  api.S.section='annotations';
  const fields=new Map([['workpaper_id','paper1'],['kind','agreed'],['note','Synthetic supporting work'],['operator_label','Synthetic preparer']]);
  api.setDialog(async(_title,_content,save)=>save(fields));
  return {api,sandbox,editor,fields};
}
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}

test('a mark binds the exact UTF-8 text and remains an unsaved draft change',async()=>{
  const {api}=harness();
  const body=api.S.draft.workpapers[0].body;
  await api.addAnnotation();
  const mark=api.S.draft.workpapers[0].annotations[0];
  assert.equal(mark.body_sha256,createHash('sha256').update(body,'utf8').digest('hex'));
  assert.equal(mark.operator_label,'Synthetic preparer');
  assert.equal(api.S.dirty,true);
  assert.equal(api.S.record.revision,1);
});

test('a route change while hashing cannot append to an old or new engagement',async()=>{
  const {api,sandbox}=harness(),gate=deferred(),oldDraft=api.S.draft;
  sandbox.crypto.subtle={digest:()=>gate.promise};
  const pending=api.addAnnotation();
  api.S.version++;
  api.S.record={id:'eng_other',status:'draft'};
  api.S.draft={workpapers:[]};
  gate.resolve(new Uint8Array(32).buffer);
  await assert.rejects(pending,/changed while anchoring/);
  assert.equal(oldDraft.workpapers[0].annotations,undefined);
  assert.equal(api.S.draft.workpapers.length,0);
});

test('text edits while hashing require reviewing the current text again',async()=>{
  const {api,sandbox}=harness(),gate=deferred();
  sandbox.crypto.subtle={digest:()=>gate.promise};
  const pending=api.addAnnotation();
  api.S.draft.workpapers[0].body='Different text';
  gate.resolve(new Uint8Array(32).buffer);
  await assert.rejects(pending,/changed while anchoring/);
  assert.equal(api.S.draft.workpapers[0].annotations,undefined);
});

test('removal cannot cross an engagement change during confirmation',async()=>{
  const {api,sandbox}=harness();
  await api.addAnnotation();
  const draft=api.S.draft,mark=draft.workpapers[0].annotations[0],gate=deferred();
  sandbox.NXT.confirm=()=>gate.promise;
  const pending=api.removeAnnotation(mark.id);
  api.S.version++;
  gate.resolve(true);
  await assert.rejects(pending,/Engagement changed/);
  assert.equal(draft.workpapers[0].annotations.length,1);
});

test('read-only display escapes all annotation text and offers no removal action',async()=>{
  const {api,editor,fields}=harness();
  fields.set('note','<img src=x onerror=alert(1)>');
  fields.set('operator_label','<svg onload=alert(1)>');
  await api.addAnnotation();
  api.S.record.status='locked';
  api.renderAnnotations();
  assert.ok(editor.innerHTML.includes('&lt;img src=x onerror=alert(1)&gt;'));
  assert.ok(editor.innerHTML.includes('&lt;svg onload=alert(1)&gt;'));
  assert.ok(!editor.innerHTML.includes('<img'));
  assert.ok(!editor.innerHTML.includes('data-action="remove-annotation"'));
});
