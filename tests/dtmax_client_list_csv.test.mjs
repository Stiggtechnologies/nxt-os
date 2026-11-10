import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const filters={search:'',year:'',sort:'client_number',direction:'asc',status:'',province:''};
const person=(i=1)=>({client_number:String(i),name:'Synthetic person '+i,historical_years:[2021,2024],pending_years:[2021],pending_years_at_import:[2021],native_historical_years:[2021,2024],source_profile:{processing_status:'Ready',province:'AB'}});
function harness({rows=[person()],alter,requestHook}={}){
 const nodes=new Map(),calls=[],downloads=[],blobs=new Map(),revoked=[],timers=[];let alive=true;
 const node=s=>{if(!nodes.has(s))nodes.set(s,{innerHTML:'',textContent:'',value:'',disabled:false,querySelector:node,querySelectorAll:()=>[]});return nodes.get(s);};
 const host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
 const local={crypto:webcrypto,Blob,TextEncoder,TextDecoder,URL:{createObjectURL:b=>{const u='blob:synthetic-'+blobs.size;blobs.set(u,b);return u;},revokeObjectURL:u=>revoked.push(u)},document:{createElement:()=>({click(){downloads.push({url:this.href,filename:this.download,blob:blobs.get(this.href)});}})},setTimeout:f=>timers.push(f)};
 vm.runInNewContext(source,local);
 async function request(path){
  calls.push(path);const index=calls.length;
  assert.match(path,/^\/api\/source-imports\/dtmax\/companies\/dtmax-test-copy\/test-copy\/recovered-native-sources\/effective\/source-clients(?:\/export)?\?/,'CSV uses roster GET only');
  const u=new URL(path,'https://synthetic.invalid'),q=u.searchParams,at=Number(q.get('offset')),needle=(q.get('search')||'').trim().toLowerCase();
  let matching=rows.filter(r=>(r.client_number+' '+r.name).toLowerCase().includes(needle)&&(!q.get('year')||r.historical_years.includes(Number(q.get('year'))))&&(!q.get('status')||r.source_profile.processing_status===q.get('status'))&&(!q.get('province')||r.source_profile.province===q.get('province')));
  if(q.get('sort')==='name')matching.sort((a,b)=>a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  if(q.get('direction')==='desc')matching.reverse();
  let p={source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:matching.length,offset:at,limit:100,available_years:[2021,2024],available_statuses:['Ready','Needs review'],available_provinces:['AB','ON'],clients:structuredClone(u.pathname.endsWith('/export')?matching:matching.slice(at,at+100))};
  if(u.pathname.endsWith('/export')){delete p.offset;delete p.limit;}
  if(alter)p=alter(p,{index,q})??p;
  if(u.pathname.endsWith('/export')){p.filters={search:q.get('search')||'',year:q.has('year')?Number(q.get('year')):null,sort:q.get('sort')||'client_number',direction:q.get('direction')||'asc',status:q.get('status'),province:q.get('province')};const hash=createHash('sha256').update(local.NXTDTMaxT1.canonicalRecoveredSource(p)).digest('hex');p={schema:'dtmax-source-client-list-export/v1',projection:p,projection_sha256:hash,observed_stable_reads_only:true,atomic_snapshot:false};}
  return requestHook?requestHook(p,{index,q}):p;
 }
 const controller=local.NXTDTMaxT1.mount(host,{historicalKind:'historical-lines',request,isCurrent:()=>alive});
 function fields(f=filters){for(const [k,v] of Object.entries(f))node('[name='+k+']').value=v;}
 const exported=()=>node('[data-dtmax-export-clients]').onclick();
 return{host,node,calls,downloads,blobs,revoked,timers,controller,fields,request,api:local.NXTDTMaxT1,exported,deactivate:()=>{alive=false;}};
}
// Parse real CSV syntax, including quoted embedded newlines, without spreadsheet coercion.
function parseCSV(s){
 assert.equal(s[0],'\uFEFF');s=s.slice(1);const records=[];let row=[],cell='',quoted=false;
 for(let i=0;i<s.length;i++){const ch=s[i];if(ch==='"'){if(quoted&&s[i+1]==='"'){cell+='"';i++;}else quoted=!quoted;}
 else if(ch===','&&!quoted){row.push(cell);cell='';}else if(ch==='\r'&&s[i+1]==='\n'&&!quoted){row.push(cell);records.push(row);row=[];cell='';i++;}else cell+=ch;}
 assert.equal(quoted,false);assert.equal(cell,'');assert.equal(row.length,0);return records;
}
async function downloaded(h){assert.equal(h.downloads.length,1);const d=h.downloads[0];assert.equal(d.filename,'dtmax-t1-source-client-list-review.csv');assert.equal(d.blob.type,'text/csv;charset=utf-8');const bytes=new Uint8Array(await d.blob.arrayBuffer());assert.deepEqual([...bytes.slice(0,3)],[239,187,191]);const r=parseCSV(new TextDecoder('utf-8',{ignoreBOM:true}).decode(bytes)),header=r.shift();return{header,rows:r.map(c=>Object.fromEntries(c.map((v,i)=>{assert.equal(v[0],"'",'every data cell has exactly the declared protective prefix');return[header[i],v.slice(1)];})))};}
test('actual mounted action exports all 5856 source profiles in two complete payloads, never source-facts',async()=>{
 const rows=Array.from({length:5856},(_,i)=>person(i+1)),before=JSON.stringify(rows),h=harness({rows});await h.controller.ready;h.fields();
 assert.match(h.host.innerHTML,/Export client list CSV/);assert.match(h.host.innerHTML,/not a migration-import CSV/);
 await h.exported();const csv=await downloaded(h);assert.equal(csv.rows.length,5856);assert.equal(csv.rows[0].source_client_number_text,'1');assert.equal(csv.rows.at(-1).source_client_number_text,'5856');assert.equal(new Set(csv.rows.map(r=>r.source_client_number_text)).size,5856);
 assert.equal(h.calls.length,3);assert.equal(h.calls.filter(u=>u.includes('source-clients/export?')).length,2);assert.equal(JSON.stringify(rows),before);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/Downloaded 5856/);
 h.timers.forEach(f=>f());assert.equal(h.revoked.length,1);h.controller.dispose();
});
test('normal applied search/year/status/province/sort filters export ALL matching profiles independent of visible page',async()=>{
 const rows=Array.from({length:205},(_,i)=>person(i+1));rows[0].source_profile.province='ON';const h=harness({rows});await h.controller.ready;
 h.fields({...filters,search:'Synthetic person',year:'2021',sort:'name',direction:'desc',status:'Ready',province:'AB'});await h.node('[data-dtmax-search]').onsubmit({preventDefault(){}});
 await h.controller.load(100);await h.exported();const csv=await downloaded(h);assert.equal(csv.rows.length,204);assert.equal(csv.rows.some(r=>r.source_client_number_text==='1'),false);assert.ok(csv.rows.every(r=>r.selected_source_year_text==='2021'&&r.province_filter_text==='AB'&&r.sort_key==='name'&&r.sort_direction==='desc'));
 assert.ok(h.calls.slice(3).every(u=>u.includes('search=Synthetic%20person&year=2021&sort=name&direction=desc&status=Ready&province=AB')));
});
test('empty matching list downloads a valid header-only UTF8 CSV after both empty scans',async()=>{
 const h=harness({rows:[]});await h.controller.ready;h.fields();await h.exported();assert.equal((await downloaded(h)).rows.length,0);assert.equal(h.calls.length,3);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/Downloaded 0/);
});
test('commas, quotes, CRLF, Unicode, leading zeros and formula-like source text roundtrip literally after one prefix removal',async()=>{
 const names=['=1+1','+SUM(A1)','-2','@command',"'already apostrophe",'\t=1+1','\r\n=1+1','Élodie, "示例"\r\nsecond line 😀'];
 const rows=names.map((name,i)=>({...person(i+1),client_number:String(i+1).padStart(5,'0'),name})),h=harness({rows});await h.controller.ready;h.fields();await h.exported();const csv=await downloaded(h);
 for(let i=0;i<rows.length;i++){assert.equal(csv.rows[i].source_name_text,names[i]);assert.equal(csv.rows[i].source_client_number_text,rows[i].client_number);assert.equal(csv.rows[i].source_historical_years_json_text,'[2021,2024]');assert.equal(csv.rows[i].source_pdf_only_years_json_text,'null');assert.match(csv.rows[i].text_cell_encoding,/remove_exactly_one/);assert.match(csv.rows[i].source_qualification,/not an atomic snapshot/);}
 assert.ok(!csv.header.some(k=>/(?:^|_)(?:sin|birth|income|balance|taxable|email)(?:_|$)/i.test(k)));
});
for(const [label,change] of [
 ['scope',p=>p.source_scope='another-source'],['module',p=>p.program='T2'],['read-only flag',p=>p.imported_source_only=false],['tax-case count',p=>p.tax_cases_created=1],['offset',p=>p.offset++],['limit',p=>p.limit=25],['total drift',p=>p.total++],['short page',p=>p.clients.pop()],['empty mid-page',p=>p.clients=[]],['metadata drift',p=>p.available_years=[2024]],['duplicate across pages',p=>p.clients[100].client_number='1'],['nontext source field',p=>p.clients[0].source_profile.province=1],['duplicate source years',p=>p.clients[0].historical_years=[2021,2021,2024]],
 ])test('inconsistent '+label+' fails closed with no partial CSV',async()=>{
 const h=harness({rows:Array.from({length:105},(_,i)=>person(i+1)),alter:(p,{index})=>{if(index===3)change(p);}});await h.controller.ready;h.fields();await h.exported();assert.equal(h.downloads.length,0);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/unavailable.*No CSV downloaded/);assert.equal(h.calls.length,3);
 });
test('same-count source row change between scans fails closed',async()=>{
 const h=harness({alter:(p,{index})=>{if(index===3)p.clients[0].name='Changed synthetic name';}});await h.controller.ready;h.fields();await h.exported();assert.equal(h.downloads.length,0);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/changed between export scans/);
});
test('auth failure partway through collection never downloads accumulated rows',async()=>{
 const h=harness({rows:Array.from({length:105},(_,i)=>person(i+1)),requestHook:(p,{index})=>{if(index===3)throw Error('401: source access required');return p;}});await h.controller.ready;h.fields();await h.exported();assert.equal(h.downloads.length,0);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/401.*No CSV downloaded/);
});
test('unapplied filter edits reject stale Export clicks without any request',async()=>{
 const h=harness();await h.controller.ready;h.fields({...filters,search:'unapplied'});await h.exported();assert.equal(h.calls.length,1);assert.equal(h.downloads.length,0);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/Apply the changed filters/);
});
test('unpreservable UTF16 source text is held rather than silently replaced in UTF8',async()=>{
 const h=harness({rows:[{...person(),name:'Synthetic \uD800'}]});await h.controller.ready;h.fields();await h.exported();assert.equal(h.downloads.length,0);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/cannot be preserved as UTF-8/);
});
test('bounded export rejects a roster above 10000 profiles',async()=>{
 const h=harness({alter:(p,{index})=>{if(index>1)p.total=10001;}});await h.controller.ready;h.fields();await h.exported();assert.equal(h.downloads.length,0);assert.equal(h.calls.length,2);
});
for(const mode of ['cancel','input','change','list navigation','dispose','detached host','different workspace'])test(mode+' while a page is pending stops further reads/download and suppresses late completion',async()=>{
 let release,started;const pending=new Promise(r=>started=r),h=harness({rows:Array.from({length:105},(_,i)=>person(i+1)),requestHook:(p,{index})=>index===2?(started(),new Promise(r=>release=()=>r(p))):p});await h.controller.ready;h.fields();const exportClick=h.node('[data-dtmax-export-clients]').onclick,operation=exportClick();await pending;
 assert.equal(h.node('[data-dtmax-export-clients]').disabled,true);assert.equal(h.node('[data-dtmax-cancel-client-export]').disabled,false);await exportClick();assert.equal(h.calls.length,2,'double-click starts no second export');
 if(mode==='cancel')h.node('[data-dtmax-cancel-client-export]').onclick();
 if(mode==='input')h.node('[data-dtmax-search]').oninput();
 if(mode==='change')h.node('[data-dtmax-search]').onchange();
 if(mode==='list navigation')await h.controller.show('list');
 if(mode==='dispose')h.controller.dispose();
 if(mode==='detached host')h.host.isConnected=false;
 if(mode==='different workspace')h.deactivate();
 const old=h.node('[data-dtmax-client-export-status]').textContent;release();await operation;assert.equal(h.calls.length,2);assert.equal(h.downloads.length,0);assert.equal(h.node('[data-dtmax-client-export-status]').textContent,old);
 if(mode==='dispose'){await exportClick();assert.equal(h.calls.length,2,'stale detached action stays inert');}
});
test('normal Find submission while export is pending cancels it without overwriting the new result',async()=>{
 let release,started;const pending=new Promise(r=>started=r),h=harness({requestHook:(p,{index})=>index===2?(started(),new Promise(r=>release=()=>r(p))):p});await h.controller.ready;h.fields();const operation=h.exported();await pending;h.fields({...filters,search:'no match'});await h.node('[data-dtmax-search]').onsubmit({preventDefault(){}});const html=h.host.innerHTML;release();await operation;assert.equal(h.host.innerHTML,html);assert.match(html,/No imported clients match/);assert.equal(h.downloads.length,0);assert.equal(h.calls.length,3);
});
test('visible progress reports both scans and cleanup revokes the final local object URL',async()=>{
 let release,started;const pending=new Promise(r=>started=r),h=harness({requestHook:(p,{index})=>index===3?(started(),new Promise(r=>release=()=>r(p))):p});await h.controller.ready;h.fields();const operation=h.exported();await pending;assert.match(h.node('[data-dtmax-client-export-status]').textContent,/roster 2 of 2 · 0 of 1/);release();await operation;await downloaded(h);h.controller.dispose();assert.equal(h.revoked.length,1);
});
for(const [label,mutate] of [
 ['content hash',p=>p.projection.clients[0].name='Tampered after hashing'],['fingerprint',p=>p.projection_sha256='0'.repeat(64)],['atomic claim',p=>p.atomic_snapshot=true],['stability qualifier',p=>p.observed_stable_reads_only=false],['schema',p=>p.schema='unknown'],['extra wrapper key',p=>p.extra=true],['extra projection key',p=>p.projection.extra=true],['filter echo',p=>p.projection.filters.search='different'],
])test('whole-payload '+label+' tamper is held without a partial download',async()=>{
 const h=harness({requestHook:(p,{index})=>{if(index>1)mutate(p);return p;}});await h.controller.ready;h.fields();await h.exported();assert.equal(h.downloads.length,0);assert.equal(h.calls.length,2);assert.match(h.node('[data-dtmax-client-export-status]').textContent,/No CSV downloaded/);
});
test('whole payload beyond serialized3MiB is rejected even when its content SHA is recomputed',async()=>{
 const h=harness();await h.controller.ready;const p=await h.request('/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/recovered-native-sources/effective/source-clients/export?search=&sort=client_number&direction=asc');p.projection.clients[0].name='X'.repeat(3*1024*1024);p.projection_sha256=createHash('sha256').update(h.api.canonicalRecoveredSource(p.projection)).digest('hex');await assert.rejects(()=>h.api.verifyClientListExport(p,filters));assert.equal(h.downloads.length,0);
});
test('fast whole-payload CSV is byte-identical to the paginated function for5856 and filtered Unicode fixtures',async()=>{
 for(const f of [filters,{...filters,search:'Synthetic person',year:'2021',status:'Ready',province:'AB',sort:'name',direction:'desc'}]){
  const h=harness({rows:Array.from({length:5856},(_,i)=>person(i+1))});await h.controller.ready;const before=h.calls.length,whole=await h.api.collectWholeClientListCsv(h.request,f);assert.equal(h.calls.length-before,2);const at=h.calls.length,paged=await h.api.collectClientListCsv(h.request,f);assert.equal(h.calls.length-at,118);assert.equal(whole.text,paged.text);h.controller.dispose();
 }
});
test('actual-like5856 full DTO recreates the exact3088065-byte accepted live CSV',async()=>{
 const file=process.env.NXT_CLIENT_EXPORT_ACTUAL_LIKE;if(!file)return;
 const p=JSON.parse(fs.readFileSync(file,'utf8')),h=harness();await h.controller.ready;const verify=await h.api.verifyClientListExport(p,filters);assert.equal(verify.projection.total,5856);
 const whole=await h.api.collectWholeClientListCsv(async()=>structuredClone(p),filters),b=Buffer.from(whole.text);assert.equal(b.length,3088065);assert.equal(createHash('sha256').update(b).digest('hex'),'a50587702204a2a48998cb8775946f807d3b3047443d1f66e881da833e4a8c08');
 const paged=await h.api.collectClientListCsv(async url=>{const at=Number(new URL(url,'https://synthetic.invalid').searchParams.get('offset'));return{...p.projection,offset:at,limit:100,clients:p.projection.clients.slice(at,at+100)};},filters);assert.equal(whole.text,paged.text);
});
