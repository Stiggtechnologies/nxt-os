import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
const clone=value=>JSON.parse(JSON.stringify(value));
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
const paper=(id,reference,title,folder='',extra={})=>({id,reference,title,folder,body:'Private evidence needle',required:false,account_ids:[],adjustment_ids:[],depends_on:[],...extra});
const fixture=()=>[paper('p3','B','ÉTUDE','Planning',{required:true}),paper('p2','a','Pending','Planning',{document_type:'placeholder',required:true,body:''}),paper('p1','A','Alpha','Evidence'),paper('p4','C','Other','')];
function harness(papers=fixture()){
 const handlers={},requests=[],toasts=[],results={innerHTML:''};let controls=[],renderCount=0,requestHandler=async()=>{throw Error('Unexpected API request');};
 const esc=value=>String(value??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
 const decode=value=>String(value??'').replace(/&#(\d+);/g,(_,n)=>String.fromCharCode(Number(n)));
 function control(tag,attrs,content=''){
  const attributes=new Map();for(const match of attrs.matchAll(/([\w-]+)(?:="([^"]*)")?/g))attributes.set(match[1],decode(match[2]??''));
  const dataset={};for(const [key,value]of attributes)if(key.startsWith('data-'))dataset[key.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]=value;
  let value=attributes.get('value')??'';if(tag==='select'){const options=[...content.matchAll(/<option value="([^"]*)"([^>]*)>/g)];value=decode((options.find(match=>match[2].includes('selected'))??options[0])?.[1]??'');}
  const node={tagName:tag.toUpperCase(),dataset,name:attributes.get('name'),value,disabled:attributes.has('disabled'),isConnected:true,selectionStart:0,selectionEnd:0,hasAttribute:key=>attributes.has(key),classList:{toggle(){}},setAttribute(key,value){attributes.set(key,value);},getAttribute:key=>attributes.get(key),focus(){document.activeElement=node;},closest(selector){return selector.startsWith('[')&&attributes.has(selector.slice(1,-1))?node:null;}};return node;
 }
 const editor={querySelector:s=>s==='[data-document-results]'?results:s==='[data-doc-filter="search"]'?controls.find(c=>c.dataset.docFilter==='search'):null,querySelectorAll:s=>s==='[data-doc-filter]'?controls.filter(c=>'docFilter'in c.dataset):s==='[data-folder]'?controls.filter(c=>'folder'in c.dataset):s==='input,select,textarea,[data-mutates]'?controls.filter(c=>['INPUT','SELECT','TEXTAREA'].includes(c.tagName)||c.hasAttribute('data-mutates')):[],contains:node=>controls.includes(node)&&node.isConnected};
 let html='';Object.defineProperty(editor,'innerHTML',{get:()=>html,set:value=>{html=value;renderCount++;controls.forEach(c=>c.isConnected=false);controls=[];for(const m of value.matchAll(/<(input|select|button)\b([^>]*?)(?:>([\s\S]*?)<\/\1>|>)/g))controls.push(control(m[1],m[2],m[3]));results.innerHTML=value.match(/<div class="card" data-document-results>([\s\S]*)<\/div>$/)?.[1]??'';}});
 const main={innerHTML:'',querySelector:()=>null,querySelectorAll:s=>s.includes('#editor input')?controls.filter(c=>['INPUT','SELECT','TEXTAREA'].includes(c.tagName)):[]};
 const document={activeElement:null,querySelector:s=>s==='.skip'?{}:s==='#main'?main:s==='#editor'?editor:s==='[data-document-results]'?results:s==='[data-doc-filter="search"]'?controls.find(c=>c.dataset.docFilter==='search'):null,querySelectorAll:()=>[],addEventListener:(name,fn)=>handlers[name]=fn};
 const sandbox={NXT:{api:(...args)=>{requests.push(args);return requestHandler(...args);},esc,modal(){throw Error('Unexpected modal');},toast:(...args)=>toasts.push(args),confirm:async()=>true},document,window:{addEventListener(){}},location:{hash:'#engagement/eng_synthetic'},history:{replaceState(){}},crypto:webcrypto,TextEncoder,Uint8Array,structuredClone,console};sandbox.parent=sandbox.window;
 const bootstrap="window.addEventListener('hashchange',()=>route());boot();";assert.equal(source.split(bootstrap).length,2);
 vm.runInNewContext(source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/,`globalThis.review={S,documentRows,resetDocumentNavigation,bindDocumentNavigation,renderDocuments,renderDocumentResults,renderEditor,renderWorkspace,loadEngagement,documentControlCurrent,updateDocumentFilter,clearDocumentFilters};})();`),sandbox);
 const api=sandbox.review,draft={name:'Synthetic engagement',kind:'internal',currency:'CAD',period_start:'2025-01-01',period_end:'2025-12-31',workpapers:papers,accounts:[],adjustments:[],groups:[],issues:[]};api.S.record={id:'eng_synthetic',revision:3,status:'draft',signoffs:[],document:clone(draft)};api.S.draft=draft;api.S.version=7;api.S.section='documents';api.renderDocuments();
 return {api,editor,results,document,requests,toasts,handlers,get controls(){return controls;},get renders(){return renderCount;},control:key=>controls.find(c=>c.dataset.docFilter===key),folder:value=>controls.find(c=>c.dataset.folder===value),clear:()=>controls.find(c=>c.hasAttribute('data-doc-clear')),input(key,value){const el=this.control(key);el.value=value;handlers.input({target:el});},change(key,value){const el=this.control(key);el.value=value;return handlers.change({target:el});},click:node=>handlers.click({target:node}),setRequest:fn=>requestHandler=fn};
}
const ids=rows=>Array.from(rows,row=>row.id);
const defaults=()=>({search:'',type:'',requirement:'',sort:'reference',folder:''});

test('combined local search, folder, type and requirement select exact rows without changing the input',()=>{
 const h=harness(),before=JSON.stringify(h.api.S.draft),nav={...defaults(),search:'  ÉTUDE  ',folder:'Planning',type:'manual',requirement:'required'};
 assert.deepEqual(ids(h.api.documentRows(h.api.S.draft.workpapers,nav)),['p3']);assert.equal(JSON.stringify(h.api.S.draft),before);assert.equal(h.api.S.dirty,false);assert.equal(h.requests.length,0);
 nav.type='placeholder';assert.deepEqual(ids(h.api.documentRows(h.api.S.draft.workpapers,nav)),[]);
});

test('Unicode lowercase and trimmed search compare metadata only without accent or normalization rewrites',()=>{
 const h=harness([paper('p1','İD','ÉTUDE','RÉVISION'),paper('p2','id','e\u0301tude','Other')]),rows=h.api.S.draft.workpapers;
 for(const search of ['  étude\u2003','RÉVISION','İd'])assert.deepEqual(ids(h.api.documentRows(rows,{...defaults(),search})),['p1']);
 assert.deepEqual(ids(h.api.documentRows(rows,{...defaults(),search:'Private evidence needle'})),[]);assert.deepEqual(ids(h.api.documentRows(rows,{...defaults(),search:'e\u0301tude'})),['p2']);
 Object.defineProperty(rows[0],'body',{get(){throw Error('Evidence body must not be read');}});assert.deepEqual(ids(h.api.documentRows(rows,{...defaults(),search:'révision'})),['p1']);
});

test('reference, title and folder sort are deterministic code-unit order with exact ID ties',()=>{
 const rows=[paper('b','a','same','same'),paper('A','A','SAME','Same'),paper('a','A','same','SAME'),paper('z','é','Z','é'),paper('c','Z','a','Z')],h=harness(rows),original=rows.map(p=>p.id);
 for(const sort of ['reference','title','folder']){
  const expected=[...rows].sort((a,b)=>{const x=String(a[sort]).trim().toLowerCase(),y=String(b[sort]).trim().toLowerCase();return (x<y?-1:x>y?1:0)||(a.id<b.id?-1:a.id>b.id?1:0);}).map(p=>p.id);
  assert.deepEqual(ids(h.api.documentRows(rows,{...defaults(),sort})),expected);
 }
 assert.deepEqual(ids(h.api.documentRows(rows,{...defaults(),sort:'reference'})),['A','a','b','c','z']);assert.deepEqual(rows.map(p=>p.id),original);
});

test('stable fallback preserves equal IDs without sorting or mutating frozen workpapers',()=>{
 const rows=[paper('same','A','First'),paper('same','a','Second')];rows.forEach(Object.freeze);Object.freeze(rows);const h=harness([]);assert.deepEqual(Array.from(h.api.documentRows(rows,defaults()),p=>p.title),['First','Second']);
});

test('manual omission, explicit manual, placeholders, optional rows and Unfiled combine honestly',()=>{
 const h=harness([paper('p1','A','Legacy'),paper('p2','B','Manual','',{document_type:'manual',required:true}),paper('p3','C','Pending','X',{document_type:'placeholder',body:'',required:true})]);
 assert.deepEqual(ids(h.api.documentRows(h.api.S.draft.workpapers,{...defaults(),type:'manual',requirement:'optional',folder:'Unfiled'})),['p1']);assert.deepEqual(ids(h.api.documentRows(h.api.S.draft.workpapers,{...defaults(),type:'placeholder',requirement:'required'})),['p3']);
});

test('at most 100 documents are searched without silent truncation',()=>{
 const h=harness([]),rows=Array.from({length:100},(_,i)=>paper(String(i),String(i),'Synthetic'));assert.equal(h.api.documentRows(rows,defaults()).length,100);assert.throws(()=>h.api.documentRows([...rows,rows[0]],defaults()),/at most 100/);assert.throws(()=>h.api.documentRows(null,defaults()),/100/);
});

test('counts, scope and explicit no-match/reset controls are rendered with escaped metadata',()=>{
 const h=harness([paper('p1','<img src=x>','<svg onload=bad>','<unsafe>')]);assert.match(h.editor.innerHTML,/Showing 1 of 1 documents/);assert.match(h.editor.innerHTML,/Evidence text is not searched/);assert.match(h.editor.innerHTML,/&#60;svg/);assert.doesNotMatch(h.editor.innerHTML,/<svg|<img/);
 h.input('search','<unmatched>');assert.match(h.results.innerHTML,/Showing 0 of 1 documents/);assert.match(h.results.innerHTML,/No documents match/);assert.match(h.results.innerHTML,/data-doc-clear/);assert.doesNotMatch(h.results.innerHTML,/<unmatched>/);
});

test('empty engagement is distinct from a no-match result',()=>{
 const h=harness([]);assert.match(h.editor.innerHTML,/Showing 0 of 0 documents/);assert.match(h.editor.innerHTML,/No documents yet/);assert.doesNotMatch(h.editor.innerHTML,/No documents match/);
});

test('actual input/change/click handlers combine filters, preserve focus and never call save/API',async()=>{
 const h=harness(),original=JSON.stringify(h.api.S.draft),search=h.control('search'),renders=h.renders;search.focus();search.selectionStart=2;search.selectionEnd=2;
 h.input('search','planning');await h.change('type','placeholder');await h.change('requirement','required');await h.change('sort','title');await h.click(h.folder('Planning'));
 assert.match(h.results.innerHTML,/Showing 1 of 4 documents/);assert.match(h.results.innerHTML,/Pending/);assert.equal(h.renders,renders);assert.equal(h.document.activeElement,search);assert.equal(search.selectionStart,2);assert.equal(h.control('search'),search);assert.equal(JSON.stringify(h.api.S.draft),original);assert.equal(h.api.S.dirty,false);assert.equal(h.requests.length,0);
});

test('clear resets query, type, requirement, sort and folder, focuses search and preserves worklist filters',async()=>{
 const h=harness();h.api.S.search='Worklist query';h.api.S.status='in_review';h.input('search','planning');await h.change('type','placeholder');await h.change('requirement','required');await h.change('sort','folder');await h.click(h.folder('Planning'));await h.click(h.clear());
 assert.deepEqual(clone(h.api.S.documentNavigation),{owner:'eng_synthetic',...defaults()});assert.equal(h.control('search').value,'');assert.equal(h.control('sort').value,'reference');assert.equal(h.document.activeElement,h.control('search'));assert.match(h.results.innerHTML,/Showing 4 of 4 documents/);assert.equal(h.api.S.search,'Worklist query');assert.equal(h.api.S.status,'in_review');assert.equal(h.requests.length,0);
});

test('locked and archived records support actual read-only navigation while mutation controls stay unavailable',async()=>{
 for(const status of ['locked','archived']){const h=harness();h.api.S.record.status=status;h.api.renderEditor();for(const c of h.controls.filter(c=>'docFilter'in c.dataset))assert.equal(c.disabled,false);assert.doesNotMatch(h.editor.innerHTML,/data-action="paper"|data-action="remove-paper"/);h.input('search','Pending');assert.match(h.results.innerHTML,/Showing 1 of 4 documents/);await h.change('type','manual');assert.match(h.results.innerHTML,/Showing 0 of 4 documents/);assert.equal(h.api.S.dirty,false);assert.equal(h.requests.length,0);}
});

test('invalid selector values and excessive query input cannot alter filters or engagement data',async()=>{
 const h=harness(),before=JSON.stringify(h.api.S.draft);await h.change('type','foreign');await h.change('requirement','all-complete');await h.change('sort','body');h.input('search','x'.repeat(257));assert.deepEqual(clone(h.api.S.documentNavigation),{owner:'eng_synthetic',...defaults()});assert.equal(JSON.stringify(h.api.S.draft),before);
});

test('detached, foreign-owner, stale-version and off-section controls cannot change the current document view',async()=>{
 for(const mode of ['detached','foreign','version','section','busy']){const h=harness(),c=h.control('search');c.value='Pending';if(mode==='detached'){h.api.renderDocuments();}if(mode==='foreign')c.dataset.docOwner='another';if(mode==='version')c.dataset.docVersion='6';if(mode==='section')h.api.S.section='annotations';if(mode==='busy')h.api.S.busy=true;h.handlers.input({target:c});assert.equal(h.api.S.documentNavigation.search,'');assert.equal(h.requests.length,0);}
 const h=harness(),oldFolder=h.folder('Planning'),oldClear=h.clear();h.api.renderDocuments();h.input('search','Pending');await h.click(oldFolder);await h.click(oldClear);assert.equal(h.api.S.documentNavigation.folder,'');assert.equal(h.api.S.documentNavigation.search,'Pending');
});

test('engagement ID changes reset document filters, same owner rerenders preserve them and old controls are rejected',()=>{
 const h=harness();h.input('search','Planning');h.api.renderDocuments();assert.equal(h.api.S.documentNavigation.search,'Planning');const old=h.control('search');h.api.S.record={...h.api.S.record,id:'eng_other'};h.api.renderDocuments();assert.deepEqual(clone(h.api.S.documentNavigation),{owner:'eng_other',...defaults()});old.value='Pending';h.handlers.input({target:old});assert.equal(h.api.S.documentNavigation.search,'');
});

test('late engagement loads cannot reset the adopted owner filters or replace current inputs',async()=>{
 const h=harness(),pending=deferred(),other={...clone(h.api.S.record),id:'eng_other'};h.setRequest(()=>pending.promise);const load=h.api.loadEngagement(7,'eng_old');h.api.S.version=8;h.api.S.record=other;h.api.S.draft=clone(other.document);h.api.renderDocuments();h.input('search','Pending');pending.resolve({...other,id:'eng_old'});await load;assert.equal(h.api.S.record.id,'eng_other');assert.equal(h.api.S.documentNavigation.search,'Pending');assert.equal(h.api.S.documentNavigation.owner,'eng_other');
});

test('adopting a valid load resets previous owner state and mismatched response fails closed',async()=>{
 const h=harness(),other={...clone(h.api.S.record),id:'eng_other'};h.input('search','Pending');h.setRequest(async()=>other);await h.api.loadEngagement(7,'eng_other');assert.equal(h.api.S.record.id,'eng_other');assert.equal(h.api.S.documentNavigation.search,'');assert.match(h.editor.innerHTML,/Showing 4 of 4 documents/);h.setRequest(async()=>({...other,id:'foreign'}));await assert.rejects(h.api.loadEngagement(7,'eng_requested'),/did not match/);assert.equal(h.api.S.record.id,'eng_other');
});


test('locked overview editing remains disabled and navigation state never enters saved preimages',()=>{
 const h=harness(),savedBefore=JSON.stringify(h.api.S.record.document),draftBefore=JSON.stringify(h.api.S.draft);h.input('search','Pending');h.api.S.record.status='locked';h.api.S.section='overview';h.api.renderEditor();
 const mutating=h.controls.filter(c=>c.hasAttribute('data-document'));assert.ok(mutating.length>0);assert.ok(mutating.every(c=>c.disabled));assert.equal(JSON.stringify(h.api.S.record.document),savedBefore);assert.equal(JSON.stringify(h.api.S.draft),draftBefore);assert.equal('documentNavigation'in h.api.S.draft,false);assert.equal(h.api.S.dirty,false);
});

test('folder button metadata and query values are escaped without changing literal source values',async()=>{
 const folder='A"><svg onload=bad>',title='Literal & title',h=harness([paper('p1','A',title,folder)]),before=JSON.stringify(h.api.S.draft);assert.doesNotMatch(h.editor.innerHTML,/<svg/);assert.match(h.editor.innerHTML,/&#34;&#62;&#60;svg/);await h.click(h.folder(folder));assert.equal(h.api.S.documentNavigation.folder,folder);h.input('search','"<svg');h.api.renderDocuments();assert.doesNotMatch(h.editor.innerHTML,/<svg/);assert.equal(JSON.stringify(h.api.S.draft),before);assert.equal(h.api.S.draft.workpapers[0].folder,folder);assert.equal(h.api.S.draft.workpapers[0].title,title);
});
