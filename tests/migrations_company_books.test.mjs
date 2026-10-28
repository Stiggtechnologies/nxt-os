import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';

const source=fs.readFileSync(new URL('../public/apps/migrations/migrations.js',import.meta.url),'utf8');
const hsh='a'.repeat(64),previewId='b'.repeat(64),controls={source_authoritative:true,parallel_test:true,live_cutover_allowed:false};
const boundary={shadow_only:true,source_authoritative:true,live_cutover_allowed:false};
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
const companyInfo={name:'Synthetic company',source_company_id:'synthetic-source',currency:'CAD',identity_verified:true,currency_verified:true};
const head=()=>({format:'nxt-company-book-head/v1',company_id:'synthetic-a',preview_id:previewId,revision:1,committed_at:'2026-10-27T00:00:00Z',reviewer_label:'Synthetic reviewer',previous_head:null,locked_through:'2025-12-31'});
const analysis=()=>({accounts:2,entries:1,lines:2,total_debit_cents:'10000',total_credit_cents:'10000',blockers:[],controls_passed:true,independent_trial_balance_accounts:2,derived_zero_accounts:0,control_provenance_complete:true,chart_of_accounts_complete:false});
const detail=()=>({company_id:'synthetic-a',stage:'import_committed',head_sha256:hsh,head:head(),previews:[{preview_id:previewId,company:companyInfo,source:{period_start:'2025-01-01',period_end:'2025-12-31'},...analysis(),stage:'preview_ready'}],previews_total:1,offset:0,limit:25,next_offset:null,has_more:false,controls});
const entry=()=>({id:'entry-a',date:'2025-12-31',source_id:'synthetic-journal-row-1',memo:'Synthetic entry',lines:[{account_id:'cash',debit_cents:'10000',credit_cents:'0',source_row:1,memo:''},{account_id:'equity',debit_cents:'0',credit_cents:'10000',source_row:2,memo:''}]});
const pageData=()=>({company_id:'synthetic-a',company:companyInfo,head:head(),offset:0,limit:100,total:1,rows:[entry()],has_more:false,next_offset:null,controls});
function harness(){
  const requests=[],toasts=[],listeners={},nodes=new Map(),node=id=>{if(!nodes.has(id))nodes.set(id,{innerHTML:'',textContent:'',dataset:{},querySelector:()=>null,querySelectorAll:()=>[],focus(){}});return nodes.get(id);};
  const main=node('#main');let handler=async()=>{throw Error('Unexpected request');};const location={hash:'#companies',origin:'https://synthetic.invalid'};
  const sandbox={NXT:{api:(path,options)=>{requests.push({path,options});return handler(path,options);},esc:v=>String(v??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';'),toast:(v)=>toasts.push(v),modal:async()=>{},key:{get:()=> 'synthetic-key'}},
    document:{querySelector:node,getElementById:id=>node('#'+id),addEventListener:(name,fn)=>listeners[name]=fn},location,window:{parent:null,addEventListener(){},confirm:()=>true},URLSearchParams,crypto:webcrypto,Uint8Array,Blob,FormData:Map,console,setTimeout};sandbox.window.parent=sandbox.window;
  const ending="  window.addEventListener('hashchange',route);route();\n})();";assert.equal(source.split(ending).length,2);
  vm.runInNewContext(source.replace(ending,'globalThis.subject={S,route,routeInfo,companies,joinCompanies,validateRoster,allBookSummaries,company,companyDataPage,bookCommand,centsInput,pagination,formatCents,nativeDialog,commitForm,companiesMarkup};})();'),sandbox);const api=sandbox.subject;
  return {api,sandbox,main,nodes,node,requests,toasts,listeners,location,setRequest:fn=>handler=fn,context(){const ctx={id:'synthetic-a',version:api.S.version,detail:detail(),data:pageData()};api.S.companyContext=ctx;return ctx;}};
}

test('company inventory fetches every financial inventory page and joins all observed companies without inferring imports',async()=>{
  const h=harness(),roster={roster_present:true,count:154,financial_imports_inferred:false,inventory_complete:false,companies:Array.from({length:154},(_,i)=>({company_id:'synthetic-'+i,company_name:'Synthetic '+i,source_navigation_ref:'selector-'+i}))};
  const books=Array.from({length:51},(_,i)=>({company_id:'synthetic-'+i,head:i===0?{company_id:'synthetic-0'}:null,previews:[],stage:'staged'}));
  h.setRequest(async path=>{if(path.endsWith('/roster'))return roster;if(path==='/api/migrations')return {...boundary,snapshots:[{id:'snapshot0',cohort_ref:'synthetic-0',company_name:'Synthetic 0',cutoff_date:'2025-12-31',report_count:3}]};const offset=Number(new URL('https://synthetic.invalid'+path).searchParams.get('offset'));return {companies:books.slice(offset,offset+50),total:51,offset,has_more:offset+50<51,controls};});
  await h.api.route();assert.equal(h.api.S.companies.length,154);assert.equal(h.requests.filter(r=>r.path.startsWith('/api/company-books?')).length,2);assert.ok(h.main.innerHTML.includes('<strong>154</strong>'));assert.ok(h.main.innerHTML.includes('<strong>1</strong>'));assert.ok(h.node('#company-list').innerHTML.includes('No committed import recorded'));assert.ok(h.node('#company-list').innerHTML.includes('Committed company journal'));assert.ok(h.node('#company-list').innerHTML.includes('No published report package'));
  assert.equal(h.requests.some(r=>r.options?.body),false);
});

test('unknown inventory does not become zero, while source evidence remains visible',async()=>{
  const h=harness();h.setRequest(async path=>{if(path==='/api/migrations')return {...boundary,snapshots:[{id:'source-a',cohort_ref:'synthetic-a',company_name:'Synthetic visible'}]};throw Error('Inventory unavailable');});await h.api.route();assert.ok(h.main.innerHTML.includes('Unknown'));assert.ok(h.main.innerHTML.includes('Unavailable stages are not reported as zero'));assert.ok(h.node('#company-list').innerHTML.includes('Import status unavailable'));assert.ok(h.node('#company-list').innerHTML.includes('Synthetic visible'));
});

test('duplicate roster references fail closed and same names never merge different companies',()=>{
  const h=harness(),row={company_id:'synthetic-a',company_name:'Synthetic',source_navigation_ref:'selector-a'};
  assert.throws(()=>h.api.validateRoster({count:2,companies:[row,row],financial_imports_inferred:false,inventory_complete:false}),/Duplicate/);
  const result=h.api.joinCompanies({companies:[row]},[{id:'s',cohort_ref:'synthetic-b',company_name:'Synthetic'}],[]);assert.equal(result.length,2);assert.equal(result[0].snapshots.length+result[1].snapshots.length,1);
});

test('inconsistent or stalled inventory pagination cannot present a completed list',async()=>{
  const h=harness();h.setRequest(async()=>({companies:[],total:1,offset:0,has_more:true,controls}));await assert.rejects(h.api.allBookSummaries(0),/no pagination progress/);
  h.setRequest(async()=>({companies:[],total:1,offset:0,has_more:false,controls}));await assert.rejects(h.api.allBookSummaries(0),/ended before/);
});

test('late company inventory results cannot replace a different route',async()=>{
  const h=harness(),gate=deferred();h.setRequest(()=>gate.promise);const pending=h.api.companies(0);h.api.S.version++;h.main.innerHTML='New company';gate.resolve({count:0,companies:[],financial_imports_inferred:false,inventory_complete:false,...boundary,snapshots:[],total:0,offset:0,has_more:false,controls});await pending;assert.equal(h.main.innerHTML,'New company');
});

test('byte-bounded book pagination retries 413 at the same offset and follows next_offset',async()=>{
  const h=harness(),ctx=h.context(),route={id:ctx.id,section:'entries',reference:'',offset:7};let attempt=0;
  h.setRequest(async path=>{if(++attempt===1){const e=Error('Budget');e.status=413;throw e;}const p=pageData();p.offset=7;p.total=50;p.has_more=true;p.next_offset=8;return p;});
  const result=await h.api.companyDataPage(ctx,route,'entries');assert.equal(result.next_offset,8);assert.ok(h.requests[0].path.includes('offset=7'));assert.ok(h.requests[1].path.includes('offset=7'));assert.ok(h.requests[1].path.includes('max_bytes=1048576'));assert.ok(h.api.pagination(route,result).includes('?offset=8'));assert.ok(!h.api.pagination(route,result).includes('offset=107'));
});

test('wrong-company and advanced-head reads cannot mix company books',async()=>{
  for(const mutate of [d=>d.company_id='synthetic-other',d=>d.head.revision=2]){const h=harness(),ctx=h.context();h.setRequest(async()=>{const p=pageData();mutate(p);return p;});await assert.rejects(h.api.companyDataPage(ctx,{id:ctx.id,section:'entries',reference:'',offset:0},'entries'),/different company|changed while loading/);}
});

test('native operations preserve one retry identity, reject edited retries and bind company plus expected head',async()=>{
  const h=harness(),ctx=h.context(),payload={expected_head:hsh,operator_label:'Synthetic operator',date:'2026-01-01',memo:'Synthetic only',lines:[]};let calls=0;
  h.setRequest(async(_path,options)=>{if(++calls===1)throw Error('Lost response');return {head_sha256:hsh,head:{...head(),commit_request_id:options.body.request_id,previous_head:hsh},controls,idempotent_replay:true,current_head_sha256:'c'.repeat(64)};});
  await assert.rejects(h.api.bookCommand(ctx,'journal','/journals',payload),/Lost response/);await assert.rejects(h.api.bookCommand(ctx,'journal','/journals',{...payload,memo:'Changed'}),/uncertain/);const result=await h.api.bookCommand(ctx,'journal','/journals',payload);
  assert.equal(JSON.stringify(h.requests[0].options.body),JSON.stringify(h.requests[1].options.body));assert.equal(h.requests[0].options.body.expected_head,hsh);assert.equal(result.current_head_sha256,'c'.repeat(64));assert.equal(ctx.detail.head_sha256,hsh);
});

test('stale operations send nothing and mismatched receipts cannot become accepted locally',async()=>{
  const h=harness(),ctx=h.context();h.api.S.version++;await assert.rejects(h.api.bookCommand(ctx,'lock','/period-lock',{}),/selected company changed/);assert.equal(h.requests.length,0);
  ctx.version=h.api.S.version;h.setRequest(async()=>({head_sha256:hsh,head:{...head(),company_id:'other'},controls}));await assert.rejects(h.api.bookCommand(ctx,'lock','/period-lock',{}),/another company/);
});

test('exact cents parsing avoids floating point and rejects exponent, signs and excess decimals',()=>{
  const h=harness();assert.equal(h.api.centsInput('9007199254740993.01'),'900719925474099301');assert.equal(h.api.formatCents('900719925474099301'),'9,007,199,254,740,993.01');for(const bad of ['1e3','-1.00','+1.00','1.001','01.00','','NaN'])assert.throws(()=>h.api.centsInput(bad));
});

test('scoped trial balance renders source and native balance controls without claiming full operational parity',async()=>{
  const h=harness();h.location.hash='#company/synthetic-a/trial-balance';h.setRequest(async path=>{if(path.includes('/trial-balance'))return {...pageData(),analysis:analysis(),source_baseline_analysis:analysis(),balances:[{account_id:'cash',name:'<img src=x>',kind:null,debit_cents:'10000',credit_cents:'0',source_debit_cents:'10000',source_credit_cents:'0',source_control_basis:{kind:'derived_zero_journal_omission'}}],native_delta:{entries:0},reconciliation:{source_baseline_unchanged:true,source_controls_passed:true,combined_balances_match_source:true}};return detail();});await h.api.route();
  assert.ok(h.main.innerHTML.includes('&#60;img src=x&#62;'));assert.ok(!h.main.innerHTML.includes('<img'));assert.ok(h.main.innerHTML.includes('Derived zero'));assert.ok(h.main.innerHTML.includes('Unmapped classification'));assert.ok(h.main.innerHTML.includes('Operational coverage still incomplete'));assert.ok(h.main.innerHTML.includes('Cutover not executed'));assert.ok(!h.main.innerHTML.includes('approval missing'));
});

test('an import preview for another company is rejected before any commit form is shown',async()=>{
  const h=harness();h.location.hash='#company/synthetic-a/import/'+previewId;h.setRequest(async path=>path.includes('/imports/')?{preview_id:previewId,preview:{company_id:'other'},controls}:detail());await h.api.route();assert.ok(h.main.innerHTML.includes('another company or import'));assert.ok(!h.main.innerHTML.includes('data-book-commit'));assert.equal(h.requests.some(r=>r.options?.body),false);
});

test('cancelled reversal shows exact opposite amounts but sends no native posting',async()=>{
  const h=harness(),ctx=h.context();let html;
  h.sandbox.NXT.modal=async(_title,render)=>{const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{});return nodes.get(k);},el={innerHTML:'',querySelector:node,querySelectorAll:()=>[]};render(el,()=>{});html=el.innerHTML;};await h.api.nativeDialog(ctx,'reverse','entry-a');assert.ok(html.includes('Exact reversing lines'));assert.ok(html.includes('Reversal debit'));assert.ok(html.includes('equity'));assert.equal(h.requests.length,0);
});

test('company route rejects malformed IDs and pagination before issuing an API request',()=>{
  const h=harness();for(const path of ['#company/bad%2Fid','#company/synthetic-a/entries?offset=-1','#company/synthetic-a/entries?offset=1.2','#company/synthetic-a/unknown']){h.location.hash=path;assert.throws(()=>h.api.routeInfo());}assert.equal(h.requests.length,0);
});

test('actual native form captures authorization before disabling controls and refreshes instead of replacing from historical replay',async()=>{
  const h=harness(),ctx=h.context(),gate=deferred(),nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{disabled:false,textContent:''});return nodes.get(k);};
  let form,closed=false;h.sandbox.NXT.modal=(_title,render)=>{const el={innerHTML:'',querySelector:node,querySelectorAll:()=>[node('button'),node('input')]};render(el,()=>{closed=true;gate.resolve();});form=node('form');return gate.promise;};
  const dialog=h.api.nativeDialog(ctx,'lock');
  h.setRequest(async(path,options)=>{assert.equal(path,'/api/company-books/synthetic-a/period-lock');assert.equal(node('input').disabled,true);assert.equal(options.body.locked_through,'2026-12-31');assert.equal(options.body.operator_label,'Synthetic operator');assert.equal(options.body.note,'Synthetic lock only');h.api.S.version++;return {head_sha256:hsh,head:{...head(),commit_request_id:options.body.request_id,previous_head:hsh},controls,idempotent_replay:true,current_head_sha256:'c'.repeat(64)};});
  await form.onsubmit({preventDefault(){},target:new Map([['operator_label','Synthetic operator'],['date','2026-12-31'],['memo','Synthetic lock only'],['confirmed','on']])});await dialog;assert.equal(closed,true);assert.equal(ctx.detail.head_sha256,hsh);assert.equal(h.requests.length,1);
});

test('a known stale-head rejection clears only the rejected operation attempt for explicit refresh',async()=>{
  const h=harness(),ctx=h.context();h.setRequest(async()=>{const e=Error('Stale expected head');e.status=409;throw e;});await assert.rejects(h.api.bookCommand(ctx,'lock','/period-lock',{expected_head:hsh}),/Stale/);assert.equal(h.api.S.attempts.size,0);
});

test('late financial page responses are ignored after company navigation',async()=>{
  const h=harness(),ctx=h.context(),gate=deferred();h.setRequest(()=>gate.promise);const pending=h.api.companyDataPage(ctx,{id:ctx.id,section:'entries',reference:'',offset:0},'entries');h.api.S.version++;gate.resolve(pageData());assert.equal(await pending,null);
});
