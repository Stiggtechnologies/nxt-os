import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
const clone=value=>JSON.parse(JSON.stringify(value));
class Fields extends Map{getAll(key){if(!this.has(key))return [];const value=this.get(key);return Array.isArray(value)?value:[value];}}
function harness(){
 const editor={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]},main={...editor},requests=[],toasts=[],previews=[],dialogs=[];
 let requestHandler=async()=>{throw Error('Unexpected synthetic request');};
 const esc=v=>String(v??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
 const sandbox={NXT:{api:(path,options)=>{requests.push({path,options});return requestHandler(path,options);},esc,toast:(...values)=>toasts.push(values),confirm:async()=>true,key:{get:()=> 'synthetic-key'},modal:async(title,render)=>{const node={innerHTML:'',querySelector:()=>({})};render(node,()=>{});previews.push({title,html:node.innerHTML});}},document:{querySelector:s=>s==='#main'?main:s==='#editor'?editor:s==='.skip'?{}:null,querySelectorAll:()=>[],addEventListener(){}},window:{addEventListener(){}},location:{hash:'#engagement/eng_synthetic'},crypto:{randomUUID:()=>webcrypto.randomUUID(),subtle:webcrypto.subtle},TextEncoder,Uint8Array,structuredClone,console};
 sandbox.parent=sandbox.window;
 const bootstrap="window.addEventListener('hashchange',()=>route());boot();";assert.equal(source.split(bootstrap).length,2);
 vm.runInNewContext(source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/,`globalThis.review={S,invoke,editPaper,viewPaper,renderDocuments,renderReview,renderAnnotations,renderFiles,addAnnotation,uploadFile,signoff,documentType,checkedText,setDialog:fn=>dialog=fn};})();`),sandbox);
 const api=sandbox.review,draft={workpapers:[{id:'p1',reference:'A',title:'Synthetic first',folder:'Planning',body:'',required:true,account_ids:['a1'],adjustment_ids:['j1'],depends_on:['p2'],future_optional:{preserved:true}},{id:'p2',reference:'B',title:'Synthetic second',folder:'Evidence',body:'Manual evidence',required:false,account_ids:[],adjustment_ids:[],depends_on:[]}],accounts:[{id:'a1',code:'100',name:'Synthetic account'}],adjustments:[{id:'j1',reference:'JE1',description:'Synthetic entry'}],groups:[],issues:[{id:'issue1',workpaper_id:'p1'}]};
 api.S.record={id:'eng_synthetic',status:'draft',revision:1,signoffs:[],document:clone(draft)};api.S.draft=draft;api.S.section='documents';
 const bytes=Uint8Array.from([1,2,3]);
 const fields=new Fields([['document_type','manual'],['reference','A'],['title','Synthetic first'],['folder','Planning'],['body',''],['required','on'],['account_ids',['a1']],['adjustment_ids',['j1']],['depends_on',['p2']],['workpaper_id','p1'],['kind','note'],['operator_label','Synthetic preparer'],['operator','Synthetic preparer'],['note','Synthetic evidence'],['file',{name:'Synthetic.bin',size:bytes.length,arrayBuffer:async()=>bytes.buffer}]]);
 function setDialog(handler){api.setDialog(async(title,html,save,...rest)=>{dialogs.push({title,html});return handler(title,html,save,...rest);});}
 setDialog(async(_title,_html,save)=>save(fields));
 return {api,sandbox,fields,editor,requests,toasts,previews,dialogs,bytes,setDialog,setRequest:fn=>requestHandler=fn,saved(){api.S.record.document=clone(api.S.draft);},placeholder(paper=api.S.draft.workpapers[0]){paper.document_type='placeholder';paper.body='';delete paper.annotations;delete paper.attachment_versions;}};
}

test('new placeholders are pending planning rows with empty evidence and preserved native references',async()=>{
 const h=harness();h.fields.set('document_type','placeholder');h.fields.set('reference','C');await h.api.editPaper();const row=h.api.S.draft.workpapers.at(-1);assert.match(row.id,/^paper-/);assert.equal(row.document_type,'placeholder');assert.equal(row.body,'');assert.equal('annotations'in row,false);assert.equal('attachment_versions'in row,false);assert.deepEqual(clone(row.account_ids),['a1']);assert.deepEqual(clone(row.depends_on),['p2']);assert.equal(h.api.S.dirty,true);assert.equal(h.api.S.record.revision,1);assert.equal(h.requests.length,0);
});

test('manual creation/edit keeps legacy omitted type and unknown optional fields without deleting annotations',async()=>{
 const h=harness();h.fields.set('reference','C');h.fields.set('body','New manual text');await h.api.editPaper();assert.equal('document_type'in h.api.S.draft.workpapers.at(-1),false);
 const row=h.api.S.draft.workpapers[0];row.document_type='manual';row.annotations=[{id:'old-mark',kind:'custom',custom_mark:{id:'custom1',code:'XY',label:'Recorded',description:'Original meaning'}}];h.saved();h.fields.set('reference','A');h.fields.set('body','Explicit new text');await h.api.editPaper('p1');const saved=h.api.S.draft.workpapers[0];assert.equal('document_type'in saved,false);assert.deepEqual(clone(saved.future_optional),{preserved:true});assert.deepEqual(clone(saved.annotations),clone(row.annotations));
});

test('empty manual and placeholder conversions retain same ID, reference, dependencies and issue links',async()=>{
 const h=harness(),issues=clone(h.api.S.draft.issues);h.fields.set('document_type','placeholder');await h.api.editPaper('p1');let row=h.api.S.draft.workpapers[0];assert.equal(row.id,'p1');assert.equal(row.reference,'A');assert.equal(row.document_type,'placeholder');assert.deepEqual(clone(row.depends_on),['p2']);assert.deepEqual(clone(h.api.S.draft.issues),issues);
 h.saved();h.fields.set('document_type','manual');h.fields.set('body','Explicit current evidence');await h.api.editPaper('p1');row=h.api.S.draft.workpapers[0];assert.equal(row.id,'p1');assert.equal('document_type'in row,false);assert.equal(row.body,'Explicit current evidence');assert.deepEqual(clone(row.adjustment_ids),['j1']);assert.deepEqual(clone(h.api.S.draft.issues),issues);
});

test('choosing placeholder never silently clears text, annotations, whitespace or immutable file versions',async()=>{
 for(const evidence of ['text','whitespace','annotation','file']){const h=harness(),row=h.api.S.draft.workpapers[0];if(evidence==='text')row.body='Original text';if(evidence==='whitespace')row.body=' ';if(evidence==='annotation')row.annotations=[{id:'old',kind:'note'}];if(evidence==='file')row.attachment_versions=[{id:'file1'}];h.saved();const before=clone(row);h.fields.set('document_type','placeholder');h.fields.set('body','');await assert.rejects(h.api.editPaper('p1'),/SAVE|immutable file versions/);assert.deepEqual(clone(h.api.S.draft.workpapers[0]),before);assert.equal(h.api.S.dirty,false);}
 const h=harness();h.fields.set('reference','C');h.fields.set('document_type','placeholder');h.fields.set('body',' ');await assert.rejects(h.api.editPaper(),/exactly empty/);assert.equal(h.api.S.draft.workpapers.length,2);
});

test('previous saved evidence must be explicitly cleared and saved before conversion',async()=>{
 for(const evidence of ['text','annotation','file']){const h=harness(),row=h.api.S.draft.workpapers[0];if(evidence==='text')row.body='Saved evidence';if(evidence==='annotation')row.annotations=[{id:'saved-mark'}];if(evidence==='file')row.attachment_versions=[{id:'saved-file'}];h.saved();row.body='';delete row.annotations;delete row.attachment_versions;h.api.S.dirty=true;h.fields.set('document_type','placeholder');await assert.rejects(h.api.editPaper('p1'),/SAVE|immutable file versions/);assert.equal(h.api.S.draft.workpapers[0].document_type,undefined);}
});

test('document fields enforce UTF-8 bytes, required values and Rust-compatible controls before mutation',async()=>{
 for(const [key,value]of [['reference','é'.repeat(33)],['title','界'.repeat(86)],['folder','Bad\u0085folder'],['body','😀'.repeat(4097)],['body','Bad\u0000text'],['title','\ud800'],['title','   ']]){const h=harness();h.fields.set(key,value);await assert.rejects(h.api.editPaper('p1'),/UTF-8 bytes/);assert.equal(h.api.S.dirty,false);}
 const h=harness();h.fields.set('reference','é'.repeat(32));h.fields.set('title','é'.repeat(128));h.fields.set('folder','界'.repeat(85));h.fields.set('body','é'.repeat(8192));await h.api.editPaper('p1');assert.equal(new TextEncoder().encode(h.api.S.draft.workpapers[0].body).length,16384);
 const multiline=harness();multiline.fields.set('body','Exact\r\ntext\tand LF\n');await multiline.api.editPaper('p1');assert.equal(multiline.api.S.draft.workpapers[0].body,'Exact\r\ntext\tand LF\n');
});

test('invalid references, duplicates, self-links and dependency cycles cannot create unsavable drafts',async()=>{
 for(const [key,values]of [['account_ids',['missing']],['account_ids',['a1','a1']],['adjustment_ids',['missing']],['depends_on',['p1']]]){const h=harness();h.fields.set(key,values);await assert.rejects(h.api.editPaper('p1'),/invalid references/);assert.equal(h.api.S.dirty,false);}
 const cycle=harness();cycle.api.S.draft.workpapers[1].depends_on=['p1'];await assert.rejects(cycle.api.editPaper('p1'),/cycle/);assert.equal(cycle.api.S.dirty,false);
 const duplicate=harness();duplicate.fields.set('reference','B');await assert.rejects(duplicate.api.editPaper('p1'),/already uses that reference/);
});

test('stale dialogs reject navigation, replaced drafts/records, deletion, concurrent row edits and locking',async()=>{
 for(const change of ['navigation','draft','record','revision','delete','row','locked']){const h=harness(),old=h.api.S.draft,before=clone(old.workpapers[0]);h.setDialog(async(_t,_html,save)=>{if(change==='navigation')h.api.S.version++;if(change==='draft')h.api.S.draft={...old};if(change==='record')h.api.S.record={...h.api.S.record};if(change==='revision')h.api.S.record.revision++;if(change==='delete')old.workpapers.shift();if(change==='row')old.workpapers[0].title='Concurrent title';if(change==='locked')h.api.S.record.status='locked';return save(h.fields);});await assert.rejects(h.api.editPaper('p1'),/changed|removed/);assert.equal(h.api.S.dirty,false);if(!['delete','row'].includes(change))assert.deepEqual(clone(old.workpapers[0]),before);}
});

test('a context change after modal save returns cannot apply a candidate to either engagement',async()=>{
 const h=harness(),old=h.api.S.draft,before=clone(old.workpapers);h.setDialog(async(_t,_html,save)=>{const candidate=await save(h.fields);h.api.S.version++;h.api.S.record={id:'other',status:'draft',revision:1};h.api.S.draft={workpapers:[]};return candidate;});await assert.rejects(h.api.editPaper('p1'),/Engagement changed/);assert.deepEqual(clone(old.workpapers),before);assert.equal(h.api.S.draft.workpapers.length,0);assert.equal(h.api.S.dirty,false);
});

test('workpaper capacity and target deletion are checked before and after the dialog',async()=>{
 const h=harness();await assert.rejects(h.api.editPaper('missing'),/no longer exists/);assert.equal(h.dialogs.length,0);
 h.api.S.draft.workpapers=Array.from({length:100},(_,i)=>({id:'p'+i,reference:'R'+i,title:'Synthetic',body:'',depends_on:[]}));await assert.rejects(h.api.editPaper(),/Maximum 100/);assert.equal(h.dialogs.length,0);
 const late=harness();late.fields.set('reference','C');late.setDialog(async(_t,_html,save)=>{while(late.api.S.draft.workpapers.length<100){const n=late.api.S.draft.workpapers.length;late.api.S.draft.workpapers.push({id:'new'+n,reference:'N'+n,title:'Synthetic',body:'',depends_on:[]});}return save(late.fields);});await assert.rejects(late.api.editPaper(),/Maximum 100/);assert.equal(late.api.S.draft.workpapers.length,100);
});

test('locked/archived documents expose no edits and direct placeholder checkpoint is rejected',async()=>{
 for(const status of ['locked','archived']){const h=harness();h.api.S.record.status=status;h.setDialog(()=>{throw Error('Locked state opened dialog');});await h.api.editPaper('p1');h.api.renderDocuments();assert.ok(!h.editor.innerHTML.includes('data-action="paper"'));assert.equal(h.api.S.dirty,false);}
 const h=harness();h.placeholder();h.saved();await assert.rejects(h.api.signoff('p1:prepared'),/Only a saved manual text/);assert.equal(h.requests.length,0);assert.equal(h.dialogs.length,0);
});

test('pending document manager, preview and review text is escaped and never offers a placeholder signoff',async()=>{
 const h=harness();h.placeholder();h.api.S.draft.workpapers=[h.api.S.draft.workpapers[0]];h.api.S.draft.workpapers[0].reference='<img src=x>';h.api.S.draft.workpapers[0].title='<svg onload=bad>';h.api.renderDocuments();assert.ok(h.editor.innerHTML.includes('Placeholder · pending'));assert.ok(h.editor.innerHTML.includes('blocks completion'));assert.ok(h.editor.innerHTML.includes('&#60;img'));assert.ok(!h.editor.innerHTML.includes('<img'));
 h.api.renderReview();assert.ok(h.editor.innerHTML.includes('Pending · convert first'));assert.ok(!h.editor.innerHTML.includes('data-action="signoff"'));
 await h.api.viewPaper('p1');assert.ok(h.previews[0].html.includes('Pending planning placeholder'));assert.ok(!h.previews[0].html.includes('No text entered.'));assert.ok(!h.previews[0].html.includes('<svg'));
 h.api.S.draft.workpapers[0].required=false;h.api.renderDocuments();assert.ok(h.editor.innerHTML.includes('Optional · warning'));
});

test('real invoke dispatch edits document types and serializes duplicate modal actions without a busy rejection',async()=>{
 const h=harness();h.fields.set('document_type','placeholder');const gate=deferred();let opened=0;h.setDialog(async(_t,_html,save)=>{opened++;await gate.promise;return save(h.fields);});const first=h.api.invoke('paper','p1');assert.equal(h.api.S.busy,true);await h.api.invoke('paper','p1');assert.equal(opened,1);gate.resolve();await first;assert.equal(h.api.S.draft.workpapers[0].document_type,'placeholder');assert.equal(h.api.S.dirty,true);assert.equal(h.api.S.busy,false);assert.equal(h.toasts.length,0);
});

test('annotation choices contain only manual text and injected placeholder selections are rejected',async()=>{
 const h=harness();h.placeholder();h.fields.set('workpaper_id','p2');await h.api.addAnnotation();const html=h.dialogs[0].html;assert.ok(!html.includes('value="p1"'));assert.ok(html.includes('value="p2"'));assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.draft.workpapers[1].annotations.length,1);
 const forced=harness();forced.placeholder();await assert.rejects(forced.api.addAnnotation(),/manual text workpaper/);assert.equal(forced.api.S.draft.workpapers[0].annotations,undefined);
 const all=harness();all.api.S.draft.workpapers.forEach(p=>all.placeholder(p));await assert.rejects(all.api.addAnnotation(),/manual text workpaper/);all.api.renderAnnotations();assert.ok(!all.editor.innerHTML.includes('data-action="annotation"'));
});

test('a document becoming a placeholder while its annotation hashes cannot receive a mark',async()=>{
 const h=harness(),gate=deferred();h.sandbox.crypto.subtle={digest:()=>gate.promise};const pending=h.api.addAnnotation();h.placeholder();gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/changed while anchoring/);assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);
});

test('upload choices exclude placeholders and direct selection does not read bytes or call an API',async()=>{
 const h=harness();h.placeholder();h.saved();let reads=0;h.fields.get('file').arrayBuffer=async()=>{reads++;return h.bytes.buffer;};await assert.rejects(h.api.uploadFile(),/manual text workpaper/);assert.ok(!h.dialogs[0].html.includes('value="p1"'));assert.ok(h.dialogs[0].html.includes('value="p2"'));assert.equal(reads,0);assert.equal(h.requests.length,0);
 h.api.S.draft.workpapers.forEach(p=>h.placeholder(p));h.saved();h.api.renderFiles();assert.ok(!h.editor.innerHTML.includes('data-action="upload-file"'));
});

test('upload revalidates exact paper/type after asynchronous file reading and ignores a late response after conversion',async()=>{
 for(const change of ['placeholder','delete']){const h=harness(),gate=deferred();h.fields.get('file').arrayBuffer=()=>gate.promise;const pending=h.api.uploadFile();if(change==='placeholder')h.placeholder();else h.api.S.draft.workpapers.shift();gate.resolve(h.bytes.buffer);await assert.rejects(pending,/changed/);assert.equal(h.requests.length,0);}
 const h=harness(),gate=deferred(),entered=deferred(),record=h.api.S.record;h.setRequest(()=>{entered.resolve();return gate.promise;});const pending=h.api.uploadFile();await entered.promise;h.placeholder();gate.resolve({id:record.id,revision:2,document:clone(record.document)});await pending;assert.equal(h.api.S.record,record);assert.equal(h.api.S.draft.workpapers[0].document_type,'placeholder');
});

test('manual upload and checkpoint labels use server-compatible byte/control checks',async()=>{
 for(const [key,value]of [['operator_label','é'.repeat(61)],['note','界'.repeat(1334)],['file',null]]){const h=harness();if(key==='file')h.fields.get('file').name='é'.repeat(81)+'.bin';else h.fields.set(key,value);await assert.rejects(h.api.uploadFile(),/UTF-8 bytes/);assert.equal(h.requests.length,0);}
 const h=harness();h.fields.set('operator','é'.repeat(61));await assert.rejects(h.api.signoff('p1:prepared'),/UTF-8 bytes/);assert.equal(h.requests.length,0);
});

test('a placeholder conversion while the checkpoint dialog is open prevents a stale request',async()=>{
 const h=harness();h.setDialog(async(_t,_html,save)=>{h.placeholder();return save(h.fields);});await assert.rejects(h.api.signoff('p1:prepared'),/document type changed/);assert.equal(h.requests.length,0);
});
