import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';

const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
const original=()=>({id:'mark_original',code:'RE',label:'Reperformed',description:'Reperform the stated synthetic procedure.'});
const defer=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
function harness(){
  const editor={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]};
  const main={...editor};
  const esc=v=>String(v??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;').replaceAll("'",'&#39;');
  const window={addEventListener(){}};
  const sandbox={NXT:{esc,api(){throw Error('No network expected');},modal(){throw Error('Unexpected modal');},toast(){},confirm:async()=>true},
    document:{querySelector:s=>s==='#editor'?editor:s==='#main'?main:s==='.skip'?{}:null,querySelectorAll:()=>[],addEventListener(){}},
    window,parent:window,location:{hash:'#worklist'},
    crypto:{randomUUID:()=>webcrypto.randomUUID(),subtle:webcrypto.subtle},TextEncoder,Uint8Array,structuredClone,console};
  const bootstrap="window.addEventListener('hashchange',()=>route());boot();";assert.equal(source.split(bootstrap).length,2);
  const runnable=source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/,`globalThis.subject={S,invoke,editTickmark,removeTickmark,addAnnotation,renderAnnotations,validateTickmarkDefinition,setDialog:fn=>dialog=fn};})();`);
  vm.runInNewContext(runnable,sandbox);
  const api=sandbox.subject;
  api.S.record={id:'eng_synthetic',status:'draft',revision:1,signoffs:[]};
  api.S.draft={workpapers:[{id:'p1',reference:'A',title:'Synthetic',body:'Synthetic text.'}],accounts:[],issues:[],tickmark_catalogue:[original()]};
  api.S.section='annotations';
  const fields=new Map([['workpaper_id','p1'],['kind','custom:mark_original'],['operator_label','Synthetic label'],['note','Synthetic evidence'],['code','RE'],['label','Updated meaning'],['description','A changed procedure for future marks.']]);
  api.setDialog(async(_title,_content,save)=>save(fields));
  return {api,sandbox,fields,editor};
}

test('applied custom definitions are copies and catalogue edits preserve recorded meaning',async()=>{
  const {api}=harness();await api.addAnnotation();
  const mark=api.S.draft.workpapers[0].annotations[0];
  assert.equal(mark.kind,'custom');assert.notEqual(mark.custom_mark,api.S.draft.tickmark_catalogue[0]);
  await api.editTickmark('mark_original');
  assert.equal(api.S.draft.tickmark_catalogue[0].label,'Updated meaning');
  assert.equal(mark.custom_mark.label,'Reperformed');
  assert.equal(api.S.record.revision,1);assert.equal(api.S.dirty,true);
});

test('removing a definition retains existing marks and prevents future use',async()=>{
  const {api}=harness();await api.addAnnotation();await api.removeTickmark('mark_original');
  assert.equal(api.S.draft.tickmark_catalogue.length,0);
  assert.equal(api.S.draft.workpapers[0].annotations[0].custom_mark.code,'RE');
  await assert.rejects(api.addAnnotation(),/no longer exists/);
  assert.equal(api.S.draft.workpapers[0].annotations.length,1);
});

test('new definitions normalize the code and stay unsaved until explicit save',async()=>{
  const {api,fields}=harness();fields.set('code','  xy-2  ');
  await api.editTickmark();const mark=api.S.draft.tickmark_catalogue[1];
  assert.equal(mark.code,'XY-2');assert.match(mark.id,/^mark-/);assert.equal(api.S.dirty,true);
  assert.equal(api.S.record.revision,1);
});

test('reserved, duplicate and malformed codes cannot alter the catalogue',async()=>{
  for(const code of ['note','ag','RC','TR','IN','CF','RE','not allowed','é','ABCDEFGHIJKLM']){
    const {api,fields}=harness();fields.set('code',code);
    await assert.rejects(api.editTickmark());assert.equal(api.S.draft.tickmark_catalogue.length,1);
    assert.equal(api.S.dirty,false);
  }
});

test('text limits are checked as UTF-8 bytes and control characters are rejected',async()=>{
  for(const [key,value] of [['label','é'.repeat(65)],['description','é'.repeat(513)],['label','Bad\nlabel'],['description','Bad\u0085description'],['description','Bad\u2028line'],['description','Bad\u2029line'],['label','   ']]){
    const {api,fields}=harness();fields.set('code','XY');fields.set(key,value);
    await assert.rejects(api.editTickmark(),/nonempty single-line text/);
    assert.equal(api.S.draft.tickmark_catalogue.length,1);
  }
});

test('catalogue and application bounds reject excess definitions and marks',async()=>{
  const {api,fields}=harness();api.S.draft.tickmark_catalogue=Array.from({length:50},(_,i)=>({id:'m'+i,code:'M'+i,label:'Synthetic',description:'Synthetic procedure.'}));
  fields.set('code','XY');await assert.rejects(api.editTickmark(),/Maximum 50/);
  assert.equal(api.S.draft.tickmark_catalogue.length,50);
  api.S.draft.tickmark_catalogue=[original()];api.S.draft.workpapers[0].annotations=Array.from({length:50},()=>({}));
  await assert.rejects(api.addAnnotation(),/Maximum 50 annotations/);
});

test('definitions changed while hashing cannot produce a mark with stale meaning',async()=>{
  for(const change of ['edit','delete']){
    const {api,sandbox}=harness(),gate=defer();sandbox.crypto.subtle={digest:()=>gate.promise};
    const pending=api.addAnnotation();
    if(change==='edit')api.S.draft.tickmark_catalogue[0].description='Changed while hashing.';
    else api.S.draft.tickmark_catalogue=[];
    gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/definition changed while anchoring/);
    assert.equal(api.S.draft.workpapers[0].annotations,undefined);
  }
});

test('a definition changed while its selection dialog is open must be reviewed again',async()=>{
  const {api,fields}=harness();
  api.setDialog(async(_t,_c,save)=>{api.S.draft.tickmark_catalogue[0].label='Different offered meaning';return save(fields);});
  await assert.rejects(api.addAnnotation(),/reopen the dialog/);
  assert.equal(api.S.draft.workpapers[0].annotations,undefined);
});

test('a removed workpaper or exhausted bound during hashing cannot retain a detached mark',async()=>{
  for(const change of ['remove','full']){
    const {api,sandbox}=harness(),gate=defer(),paper=api.S.draft.workpapers[0];sandbox.crypto.subtle={digest:()=>gate.promise};
    const pending=api.addAnnotation();
    if(change==='remove')api.S.draft.workpapers=[];else paper.annotations=Array.from({length:50},()=>({}));
    gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/changed while anchoring|Maximum 50/);
    assert.equal(paper.annotations?.length,change==='remove'?undefined:50);
  }
});

test('a route change during the definition dialog cannot mutate either engagement',async()=>{
  const {api,fields}=harness(),old=api.S.draft;
  api.setDialog(async(_t,_c,save)=>{api.S.version++;api.S.draft={workpapers:[]};return save(fields);});
  await assert.rejects(api.editTickmark('mark_original'),/Engagement changed/);
  assert.equal(old.tickmark_catalogue[0].label,'Reperformed');
  assert.equal(api.S.draft.tickmark_catalogue,undefined);
});

test('a concurrent definition edit or deletion invalidates an open editor',async()=>{
  for(const action of ['edit','delete']){
    const {api,fields}=harness();
    api.setDialog(async(_t,_c,save)=>{if(action==='edit')api.S.draft.tickmark_catalogue[0].label='Concurrent edit';else api.S.draft.tickmark_catalogue=[];return save(fields);});
    await assert.rejects(api.editTickmark('mark_original'),/Definition changed/);
    assert.equal(api.S.dirty,false);
  }
});

test('a route change or concurrent edit during removal cannot remove the definition',async()=>{
  for(const action of ['route','edit']){
    const {api,sandbox}=harness(),gate=defer();sandbox.NXT.confirm=()=>gate.promise;
    const pending=api.removeTickmark('mark_original');
    if(action==='route')api.S.version++;else api.S.draft.tickmark_catalogue[0].label='Concurrent edit';
    gate.resolve(true);await assert.rejects(pending,/changed/);
    assert.equal(api.S.draft.tickmark_catalogue.length,1);
  }
});

test('locked and archived catalogues expose no mutations',async()=>{
  for(const status of ['locked','archived']){
    const {api,editor}=harness();api.S.record.status=status;
    api.setDialog(()=>{throw Error('Read-only state opened a modal');});
    await api.editTickmark();await api.removeTickmark('mark_original');await api.addAnnotation();api.renderAnnotations();
    assert.equal(api.S.dirty,false);assert.ok(!editor.innerHTML.includes('data-mutates'));
    assert.ok(!editor.innerHTML.includes('data-action="edit-tickmark"'));
  }
});

test('custom catalogue and recorded definition text is escaped separately',async()=>{
  const {api,editor}=harness();await api.addAnnotation();
  api.S.draft.tickmark_catalogue[0].label='<svg onload=alert(1)>';
  api.S.draft.workpapers[0].annotations[0].custom_mark.description='<img src=x onerror=alert(1)>';
  api.renderAnnotations();assert.ok(editor.innerHTML.includes('&lt;svg'));
  assert.ok(editor.innerHTML.includes('&lt;img'));assert.ok(!editor.innerHTML.includes('<img'));
  assert.ok(editor.innerHTML.includes('RE · Reperformed'));
});

test('built-in marks retain the legacy shape without an unused custom field',async()=>{
  const {api,fields}=harness();fields.set('kind','agreed');await api.addAnnotation();
  const mark=api.S.draft.workpapers[0].annotations[0];assert.equal(mark.kind,'agreed');assert.equal('custom_mark' in mark,false);
});

test('annotation evidence and operator labels must meet server byte and control limits before appending',async()=>{
  for(const [key,value] of [['operator_label','é'.repeat(65)],['operator_label','Bad\nlabel'],['note','é'.repeat(2049)],['note','Bad\u0000evidence']]){
    const {api,fields}=harness();fields.set(key,value);
    await assert.rejects(api.addAnnotation(),/UTF-8 bytes/);
    assert.equal(api.S.draft.workpapers[0].annotations,undefined);assert.equal(api.S.dirty,false);
  }
  const {api,fields}=harness();fields.set('note','Valid\r\nmultiline\tevidence');
  await api.addAnnotation();assert.equal(api.S.draft.workpapers[0].annotations[0].note,'Valid\r\nmultiline\tevidence');
});

test('real action dispatch can edit, apply and remove definitions under its busy lock',async()=>{
  const {api}=harness();
  await api.invoke('edit-tickmark','mark_original');
  assert.equal(api.S.draft.tickmark_catalogue[0].label,'Updated meaning');assert.equal(api.S.busy,false);
  await api.invoke('annotation','');assert.equal(api.S.draft.workpapers[0].annotations.length,1);
  await api.invoke('remove-tickmark','mark_original');assert.equal(api.S.draft.tickmark_catalogue.length,0);
  assert.equal(api.S.draft.workpapers[0].annotations[0].custom_mark.label,'Updated meaning');
  assert.equal(api.S.busy,false);
});

test('action dispatch serializes repeated catalogue mutations while a dialog is open',async()=>{
  const {api,fields}=harness(),gate=defer();let dialogs=0;
  api.setDialog(async(_t,_c,save)=>{dialogs++;await gate.promise;return save(fields);});
  const pending=api.invoke('edit-tickmark','mark_original');assert.equal(api.S.busy,true);
  await api.invoke('remove-tickmark','mark_original');assert.equal(dialogs,1);
  gate.resolve();await pending;assert.equal(api.S.draft.tickmark_catalogue.length,1);assert.equal(api.S.busy,false);
});
