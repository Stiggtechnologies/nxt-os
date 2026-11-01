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
 const sandbox={NXT:{api:(path,options)=>{requests.push({path,options});return requestHandler(path,options);},esc,toast:(...values)=>toasts.push(values),confirm:async()=>true,key:{get:()=> 'synthetic-key'},modal:async(title,render)=>{const node={innerHTML:'',querySelector:()=>({})};render(node,()=>{});previews.push({title,html:node.innerHTML});}},document:{querySelector:s=>s==='#main'?main:s==='#editor'?editor:s==='.skip'?{}:null,querySelectorAll:()=>[],addEventListener(){}},window:{addEventListener(){}},location:{hash:'#engagement/eng_synthetic'},crypto:{randomUUID:()=>webcrypto.randomUUID(),subtle:webcrypto.subtle},TextEncoder,Uint8Array,structuredClone,URL,console};
 sandbox.parent=sandbox.window;
 const bootstrap="window.addEventListener('hashchange',()=>route());boot();";assert.equal(source.split(bootstrap).length,2);
 vm.runInNewContext(source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/,`globalThis.review={S,invoke,editPaper,viewPaper,renderDocuments,renderReview,renderAnnotations,renderFiles,addAnnotation,uploadFile,signoff,documentType,checkedText,checkedReferenceUrl,externalReferenceMarkup,documentRows,setDialog:fn=>dialog=fn};})();`),sandbox);
 const api=sandbox.review,draft={workpapers:[{id:'p1',reference:'A',title:'Synthetic first',folder:'Planning',body:'',required:true,account_ids:['a1'],adjustment_ids:['j1'],depends_on:['p2'],future_optional:{preserved:true}},{id:'p2',reference:'B',title:'Synthetic second',folder:'Evidence',body:'Manual evidence',required:false,account_ids:[],adjustment_ids:[],depends_on:[]}],accounts:[{id:'a1',code:'100',name:'Synthetic account'}],adjustments:[{id:'j1',reference:'JE1',description:'Synthetic entry'}],groups:[],issues:[{id:'issue1',workpaper_id:'p1'}]};
 api.S.record={id:'eng_synthetic',status:'draft',revision:1,signoffs:[],document:clone(draft)};api.S.draft=draft;api.S.section='documents';
 const bytes=Uint8Array.from([1,2,3]);
 const fields=new Fields([['reference_url',''],['document_type','manual'],['reference','A'],['title','Synthetic first'],['folder','Planning'],['body',''],['required','on'],['account_ids',['a1']],['adjustment_ids',['j1']],['depends_on',['p2']],['workpaper_id','p1'],['kind','note'],['operator_label','Synthetic preparer'],['operator','Synthetic preparer'],['note','Synthetic evidence'],['file',{name:'Synthetic.bin',size:bytes.length,arrayBuffer:async()=>bytes.buffer}]]);
 function setDialog(handler){api.setDialog(async(title,html,save,...rest)=>{dialogs.push({title,html});return handler(title,html,save,...rest);});}
 setDialog(async(_title,_html,save)=>save(fields));
 return {api,sandbox,fields,editor,requests,toasts,previews,dialogs,bytes,setDialog,setRequest:fn=>requestHandler=fn,saved(){api.S.record.document=clone(api.S.draft);},placeholder(paper=api.S.draft.workpapers[0]){paper.document_type='placeholder';paper.body='';delete paper.annotations;delete paper.attachment_versions;}};
}

const external=(h,paper=h.api.S.draft.workpapers[0])=>{paper.document_type='external_reference';paper.reference_url='https://example.invalid/Exact?x=1&y=2#Part';paper.body='Native reviewed note';h.saved();return paper;};

test('current-main tags and link labels survive conversion and external metadata edits',async()=>{
 const h=harness(),paper=h.api.S.draft.workpapers[0];paper.tags=['review','tax'];paper.link_label='Prior year lead';h.saved();
 h.fields.set('document_type','external_reference');h.fields.set('reference_url','HTTPS://Example.INVALID:443/Exact');await h.api.editPaper('p1');
 let saved=h.api.S.draft.workpapers[0];assert.deepEqual(clone(saved.tags),['review','tax']);assert.equal(saved.link_label,'Prior year lead');
 assert.equal(saved.reference_url,'HTTPS://Example.INVALID:443/Exact');assert.equal(h.api.S.record.document.workpapers[0].document_type,undefined);
 h.saved();h.fields.set('reference_url','https://example.invalid/changed');h.fields.set('body','Deliberately updated native note');await h.api.editPaper('p1');
 saved=h.api.S.draft.workpapers[0];assert.deepEqual(clone(saved.tags),['review','tax']);assert.equal(saved.link_label,'Prior year lead');assert.equal(saved.body,'Deliberately updated native note');
 assert.equal(h.api.S.record.document.workpapers[0].reference_url,'HTTPS://Example.INVALID:443/Exact');assert.equal(h.requests.length,0);
});

test('creating external reference preserves raw HTTPS spelling, notes, planning refs and remains unsaved',async()=>{
 const h=harness();h.fields.set('document_type','external_reference');h.fields.set('reference','C');h.fields.set('reference_url','HTTPS://Example.INVALID:443/Exact?x=1&y=2#Part');h.fields.set('body','Native note');await h.api.invoke('paper');const row=h.api.S.draft.workpapers.at(-1);assert.equal(row.document_type,'external_reference');assert.equal(row.reference_url,'HTTPS://Example.INVALID:443/Exact?x=1&y=2#Part');assert.equal(row.body,'Native note');assert.deepEqual(clone(row.account_ids),['a1']);assert.deepEqual(clone(row.depends_on),['p2']);assert.equal(h.api.S.dirty,true);assert.equal(h.requests.length,0);assert.equal(h.api.S.busy,false);
});

test('strict URL validation rejects schemes, credentials, empty authority, whitespace, controls and backslashes',()=>{
 const h=harness();for(const url of ['',null,1,'http://example.invalid','javascript:alert(1)','data:text/html,x','//example.invalid','https:example.invalid','https:///example.invalid','https://','https://@example.invalid','https://u@example.invalid','https://u:p@example.invalid','https://example.invalid\\evil','https://example.invalid/a b',' https://example.invalid','https://example.invalid/\n','https://example.invalid/\u0085','https://example.invalid/\u2003','https://[bad]','https://example.invalid:99999','https://example.invalid/'+ 'é'.repeat(1024)])assert.throws(()=>h.api.checkedReferenceUrl(url),undefined,JSON.stringify(url));
 for(const url of ['https://example.invalid','HTTPS://Example.INVALID:443/Exact?x=1&y=2#Part','https://[::1]:8443/a','https://éxample.invalid/path'])assert.equal(h.api.checkedReferenceUrl(url),url);
});

test('manual-to-external conversion preserves saved text and identities, while changed notes require a separate saved edit',async()=>{
 const h=harness();h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid');await h.api.editPaper('p1');const row=h.api.S.draft.workpapers[0];assert.equal(row.id,'p1');assert.equal(row.body,'');assert.equal(row.document_type,'external_reference');assert.deepEqual(clone(row.adjustment_ids),['j1']);assert.deepEqual(clone(row.depends_on),['p2']);assert.deepEqual(clone(row.future_optional),{preserved:true});
 const changed=harness();changed.fields.set('document_type','external_reference');changed.fields.set('reference_url','https://example.invalid');changed.fields.set('body','Different from saved text');await assert.rejects(changed.api.editPaper('p1'),/preserve the saved body/);assert.equal(changed.api.S.dirty,false);
});

test('saved annotations or immutable files cannot disappear inside external conversion',async()=>{
 for(const kind of ['annotations','attachment_versions']){const h=harness();h.api.S.draft.workpapers[0][kind]=[{id:'saved'}];h.saved();h.api.S.draft.workpapers[0][kind]=[];h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid');const before=JSON.stringify(h.api.S.draft);await assert.rejects(h.api.editPaper('p1'),/Remove marks and SAVE first|immutable file/);assert.equal(JSON.stringify(h.api.S.draft),before);assert.equal(h.api.S.dirty,false);}
});

test('existing external references cannot convert away even if the URL is explicitly cleared or omitted',async()=>{
 for(const type of ['manual','placeholder'])for(const url of ['',null]){const h=harness(),row=external(h),before=clone(row);h.fields.set('document_type',type);if(url===null)h.fields.delete('reference_url');else h.fields.set('reference_url',url);h.fields.set('body',row.body);await assert.rejects(h.api.editPaper('p1'),/retain their type and URL/);assert.deepEqual(clone(h.api.S.draft.workpapers[0]),before);assert.equal(h.api.S.dirty,false);}
});

test('placeholder-to-external gains an explicit URL and native note without losing planning refs',async()=>{
 const h=harness();h.placeholder();h.saved();h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid');h.fields.set('body','Reviewed native note');await h.api.editPaper('p1');assert.equal(h.api.S.draft.workpapers[0].document_type,'external_reference');assert.deepEqual(clone(h.api.S.draft.workpapers[0].account_ids),['a1']);assert.equal(h.api.S.draft.issues[0].workpaper_id,'p1');
});

test('nonexternal URL is rejected without silent removal and legacy manual output keeps absent fields',async()=>{
 const h=harness();h.fields.set('reference_url','https://example.invalid');await assert.rejects(h.api.editPaper('p1'),/Only external references/);assert.equal(h.api.S.dirty,false);h.fields.set('reference_url','');await h.api.editPaper('p1');assert.equal('reference_url'in h.api.S.draft.workpapers[0],false);assert.equal('document_type'in h.api.S.draft.workpapers[0],false);
});

test('editor retains URL across type selection, renders escaped field values and never auto-loads destination content',async()=>{
 const h=harness(),row=external(h);row.reference_url='https://example.invalid/?x="<script>"&y=1';h.saved();h.fields.set('reference_url',row.reference_url);h.fields.set('document_type','external_reference');h.fields.set('body',row.body);await h.api.editPaper('p1');const html=h.dialogs[0].html;assert.match(html,/name="reference_url"/);assert.match(html,/&#34;&#60;script&#62;&#34;/);assert.doesNotMatch(html,/<script>|<iframe|<object|<embed/);assert.equal(h.requests.length,0);
});

test('external read-only preview opens only explicit safe HTTPS with isolation and escaped unverified scope',async()=>{
 const h=harness(),row=external(h);row.body='<svg onload=bad>';await h.api.viewPaper('p1');const html=h.previews[0].html;assert.match(html,/target="_blank" rel="noopener noreferrer" referrerpolicy="no-referrer"/);assert.match(html,/External content is not retained or verified/);assert.match(html,/&#60;svg/);assert.doesNotMatch(html,/<svg|<iframe|<object|<embed/);assert.equal(h.requests.length,0);
 for(const url of ['javascript:alert(1)','https://@example.invalid','https://example.invalid\\bad',null]){row.reference_url=url;assert.doesNotMatch(h.api.externalReferenceMarkup(row),/href=/);}
});

test('document manager and review expose the distinct type, external filter, counts and metadata checkpoints',()=>{
 const h=harness();external(h);h.api.renderDocuments();assert.match(h.editor.innerHTML,/External HTTPS reference/);assert.match(h.editor.innerHTML,/value="external_reference"/);assert.match(h.editor.innerHTML,/Showing 2 of 2 documents/);const rows=h.api.documentRows(h.api.S.draft.workpapers,{type:'external_reference',sort:'reference'});assert.equal(rows.length,1);h.api.renderReview();assert.match(h.editor.innerHTML,/saved URL and note only/);assert.match(h.editor.innerHTML,/data-id="p1:prepared"/);assert.equal(h.requests.length,0);
});

test('external marks and file choices remain excluded from manual-only operations',async()=>{
 const h=harness();external(h);h.fields.set('workpaper_id','p1');await assert.rejects(h.api.addAnnotation(),/manual text/);await assert.rejects(h.api.uploadFile(),/manual text/);assert.equal(h.requests.length,0);assert.equal(h.api.S.dirty,false);
});

test('external checkpoint requires saved nonempty note and stale URL/type/owner cannot send a request',async()=>{
 const empty=harness();external(empty).body=' ';empty.saved();await assert.rejects(empty.api.signoff('p1:prepared'),/native review note/);assert.equal(empty.requests.length,0);
 for(const change of [h=>h.api.S.record.id='foreign',h=>h.api.S.version++,h=>h.api.S.draft.workpapers[0].reference_url='https://changed.invalid',h=>h.api.S.draft.workpapers[0].document_type='placeholder']){const h=harness();external(h);h.setDialog(async(_t,_html,save)=>{change(h);return save(h.fields);});await assert.rejects(h.api.signoff('p1:prepared'),/changed/);assert.equal(h.requests.length,0);}
});

test('external checkpoint records only revision-bound metadata and explicitly disclaims retained destination evidence',async()=>{
 const h=harness();external(h);h.setRequest(async()=>({...clone(h.api.S.record),revision:2}));await h.api.invoke('signoff','p1:prepared');assert.equal(h.requests.length,1);assert.deepEqual(clone(h.requests[0].options.body),{expected_revision:1,workpaper_id:'p1',stage:'prepared',operator:'Synthetic preparer',note:'Synthetic evidence'});assert.match(h.dialogs[0].html,/destination content is not fetched, retained or verified/);assert.equal(h.toasts.some(args=>args[1]===true),false);
});

test('external edit has owner/revision/draft/row guards and rejects late candidates without mutating either file',async()=>{
 for(const change of [h=>h.api.S.record.id='foreign',h=>h.api.S.record.revision++,h=>h.api.S.version++,h=>h.api.S.record.status='locked',h=>h.api.S.draft={...h.api.S.draft},h=>h.api.S.draft.workpapers.splice(0,1)]){const h=harness();external(h);h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid/new');h.fields.set('body','Native reviewed note');h.setDialog(async(_t,_html,save)=>{change(h);return save(h.fields);});await assert.rejects(h.api.editPaper('p1'),/changed|removed/);assert.equal(h.api.S.dirty,false);assert.equal(h.requests.length,0);}
 const h=harness();external(h);h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid/new');h.fields.set('body','Native reviewed note');const before=JSON.stringify(h.api.S.draft);h.setDialog(async(_t,_html,save)=>{const next=await save(h.fields);h.api.S.version++;return next;});await assert.rejects(h.api.editPaper('p1'),/changed/);assert.equal(JSON.stringify(h.api.S.draft),before);
});


test('actual editor setup preserves exact unchanged saved bytes across native LF/CRLF/bareCR normalization',async()=>{
 for(const body of ['\nA\r\nB\rC','\r\nLeading\rTail','\rLeading','A😀e\u0301\r\nZ']){
  const h=harness();h.api.S.draft.workpapers[0].body=body;h.saved();h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid');let setupCalled=false;
  h.setDialog(async(_title,_html,save,_label,setup)=>{
   const normalized=body.replace(/\r\n?/g,'\n'),native={value:normalized.startsWith('\n')?normalized.slice(1):normalized};
   assert.equal(typeof setup,'function');setup({querySelector:selector=>selector==='[name="body"]'?native:null});setupCalled=true;assert.equal(native.value,normalized);
   // Browser FormData reads the normalized live textarea value, not its original HTML.
   h.fields.set('body',native.value);return save(h.fields);
  });
  await h.api.invoke('paper','p1');assert.equal(setupCalled,true);assert.equal(h.api.S.draft.workpapers[0].body,body);assert.equal(h.api.S.draft.workpapers[0].document_type,'external_reference');assert.equal(h.toasts.some(args=>args[1]===true),false);assert.equal(h.requests.length,0);
 }
});

test('visible note edits remain deliberate changes and cannot be hidden inside manual-to-external conversion',async()=>{
 const h=harness();h.api.S.draft.workpapers[0].body='\nA\r\nB\rC';h.saved();h.fields.set('document_type','external_reference');h.fields.set('reference_url','https://example.invalid');
 h.setDialog(async(_title,_html,save,_label,setup)=>{const native={value:''};setup({querySelector:()=>native});native.value+=' changed';h.fields.set('body',native.value);return save(h.fields);});await assert.rejects(h.api.editPaper('p1'),/preserve the saved body/);assert.equal(h.api.S.draft.workpapers[0].body,'\nA\r\nB\rC');assert.equal(h.api.S.dirty,false);
});


test('URL editor avoids native URL-input trimming and rejects visible whitespace/newlines instead of sanitizing them',async()=>{
 for(const value of [' https://example.invalid ','https://example.invalid/\r\npath','\nhttps://example.invalid']){
  const h=harness();h.fields.set('document_type','external_reference');h.setDialog(async(_title,html,save)=>{
   assert.match(html,/<textarea name="reference_url" rows="2" maxlength="2048" inputmode="url"/);assert.doesNotMatch(html,/<input name="reference_url"/);
   // Native textarea/FormData normalizes CRLF to LF, retaining invalid whitespace.
   h.fields.set('reference_url',value.replace(/\r\n?/g,'\n'));return save(h.fields);
  });await assert.rejects(h.api.editPaper('p1'));assert.equal(h.api.S.dirty,false);assert.equal(h.requests.length,0);assert.equal('reference_url'in h.api.S.draft.workpapers[0],false);
 }
});
