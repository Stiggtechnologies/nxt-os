/* Read-only imported personal T1 source records. Never creates/calculates a tax case. */
(function () {
  'use strict';
  const SCOPE='dtmax-test-copy', BASE='/api/source-imports/dtmax/companies/'+SCOPE+'/test-copy/';
  const esc=v=>String(v??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
  const lines=['15000','23600','26000','42000','48400','48500'];
  const labels=['Total income','Net income','Taxable income','Net federal tax','Refund','Balance owing'];
  const kinds={'historical-lines':'Historical totals','afr-slips':'CRA slips','family-files':'Family files'};
  const integer=v=>Number.isSafeInteger(v)&&v>=0;
  function cents(value) {
    if(value===null)return 'Absent';
    if(typeof value!=='string'||!/^(-?[1-9]\d*|0)$/.test(value))throw Error('Invalid source amount');
    const n=BigInt(value);if(n< -9223372036854775808n||n>9223372036854775807n)throw Error('Source amount outside range');
    const a=n<0n?-n:n,whole=(a/100n).toString().replace(/\B(?=(\d{3})+(?!\d))/g,',');
    return (n<0n?'-$':'$')+whole+'.'+(a%100n).toString().padStart(2,'0');
  }
  function validatePage(p,offset) {
    if(!p||p.source_scope!==SCOPE||p.program!=='T1'||p.imported_source_only!==true||p.tax_cases_created!==0||p.offset!==offset||!integer(p.total)||!Array.isArray(p.clients)||p.clients.length>100||p.clients.length>p.total||!Array.isArray(p.available_years))throw Error('Imported T1 list could not be verified');
    if(p.available_years.some(y=>!Number.isInteger(y)||y<1990||y>2100)||new Set(p.available_years).size!==p.available_years.length)throw Error('Invalid available years');
    const seen=new Set();
    for(const c of p.clients){
      if(!c||!/^\d{1,12}$/.test(c.client_number)||seen.has(c.client_number)||typeof c.name!=='string'||!c.name.trim()||!Array.isArray(c.historical_years)||!Array.isArray(c.pending_years))throw Error('Invalid imported client');
      seen.add(c.client_number);
      if([...c.historical_years,...c.pending_years,...p.available_years].some(y=>!Number.isInteger(y)||y<1990||y>2100)||c.pending_years.some(y=>!c.historical_years.includes(y)))throw Error('Invalid source years');
    }
    return p;
  }
  function tree(node,depth=0) {
    if(!node||depth>12||typeof node.tag!=='string'||!Array.isArray(node.children))throw Error('Invalid slip source fields');
    const label=node.tag.split('}').at(-1),box=node.attributes?.boxNumber;
    return '<li><strong>'+esc(label)+(box?' · Box '+esc(box):'')+'</strong>'+ (node.text===null?'':': '+esc(node.text))+(node.children.length?'<ul>'+node.children.map(c=>tree(c,depth+1)).join('')+'</ul>':'')+'</li>';
  }
  function recordsHtml(kind,records) {
    if(!records.length)return '<p>No records of this kind are imported for this client.</p>';
    if(kind==='historical-lines')return '<p>Absent means no source value; $0.00 is an explicit zero. Imported totals have not been recalculated.</p><div class="table-wrap"><table><thead><tr><th>Year</th>'+lines.map((l,i)=>'<th>'+labels[i]+' ('+l+')</th>').join('')+'<th>Verification</th></tr></thead><tbody>'+records.map(r=>{
      if(!Number.isInteger(r.tax_year)||!r.values_cents||!['vendor_static_verified','pending_year_verification','pending_module_verification'].includes(r.mapping_state))throw Error('Invalid source figures');
      return '<tr><td>'+r.tax_year+'</td>'+lines.map(l=>'<td>'+esc(cents(r.values_cents[l]))+'</td>').join('')+'<td>'+esc(r.mapping_state==='vendor_static_verified'?'Vendor mapping checked; source totals':'Mapping needs verification')+'</td></tr>';
    }).join('')+'</tbody></table></div>';
    if(kind==='family-files')return records.map(r=>'<p>Family file '+esc(r.family_file_id)+' · '+esc(r.tax_year)+' · Member client IDs: '+esc((r.member_client_numbers||[]).join(', ')||'Not established')+'</p>').join('');
    return '<p>Source occurrences are preserved separately; they are not deduplicated economic slips.</p>'+records.map(r=>'<details><summary>'+esc(r.slip_kind)+' · '+esc(r.tax_year===null?'Missing tax year':r.tax_year)+'</summary><p>Source folder year: '+esc(r.source_folder_year)+' (not an inferred slip year)</p><ul>'+tree(r.vendor_field_tree)+'</ul></details>').join('');
  }
  function mount(host,{request,isCurrent=()=>true}) {
    let sequence=0,offset=0,search='',year='',activeClient='',kind='historical-lines',factOffset=0;
    const current=n=>host.isConnected!==false&&isCurrent()&&n===sequence;
    async function load(next=0){
      const n=++sequence;offset=next;activeClient='';
      host.innerHTML='<div class="card"><h2>T1 clients imported from DT Max</h2><p>Loading imported clients…</p></div>';
      try{
        const p=validatePage(await request(BASE+'source-clients?offset='+offset+'&limit=100&search='+encodeURIComponent(search)+(year?'&year='+year:'')),offset);
        if(!current(n))return;
        host.innerHTML='<div class="card"><h2>T1 clients imported from DT Max</h2><p>Firm-wide personal T1 source list, independent of the company selector. These are preserved source records; NXT preparation workpapers are listed separately below.</p><form data-dtmax-search class="list-filters"><label>Find imported client<input name="search" type="search" maxlength="100" value="'+esc(search)+'" placeholder="Name or DT Max client number"></label><label>Imported year<select name="year"><option value="">All years</option>'+[...p.available_years].sort((a,b)=>b-a).map(y=>'<option'+(String(y)===year?' selected':'')+'>'+y+'</option>').join('')+'</select></label><button class="btn" type="submit">Search imported clients</button></form><p>'+ (p.total?offset+1:0)+'–'+(offset+p.clients.length)+' of '+p.total+' clients</p><div class="table-wrap"><table><thead><tr><th>DT Max client #</th><th>Name</th><th>Imported historical years</th><th></th></tr></thead><tbody>'+p.clients.map(c=>'<tr><td>'+esc(c.client_number)+'</td><td>'+esc(c.name)+'</td><td>'+esc(c.historical_years.join(', ')||'No decoded historical totals')+(c.pending_years.length?'<div class="muted">Needs verification: '+esc(c.pending_years.join(', '))+'</div>':'')+'</td><td><button class="btn sm" type="button" data-imported-client="'+esc(c.client_number)+'">Open imported T1</button></td></tr>').join('')+'</tbody></table></div>'+(!p.total?'<p>No imported clients match these filters.</p>':'')+'<button class="btn" type="button" data-dtmax-prev '+(!offset?'disabled':'')+'>Previous</button> <button class="btn" type="button" data-dtmax-next '+(offset+p.clients.length>=p.total?'disabled':'')+'>Next</button><div data-dtmax-details></div></div>';
        host.querySelector('[data-dtmax-search]').onsubmit=e=>{e.preventDefault();search=host.querySelector('[name=search]').value;year=host.querySelector('[name=year]').value;return load(0);};
        host.querySelector('[data-dtmax-prev]').onclick=()=>load(Math.max(0,offset-100));host.querySelector('[data-dtmax-next]').onclick=()=>load(offset+100);
        host.querySelectorAll('[data-imported-client]').forEach(b=>b.onclick=()=>{activeClient=b.dataset.importedClient;kind='historical-lines';return facts(0);});
      }catch(e){if(current(n))host.innerHTML='<div class="card"><h2>T1 imported clients unavailable</h2><p role="alert">'+esc(e.message)+'</p><p>Unavailable data is not an empty client list. Firm-wide source access is required.</p><button class="btn" data-dtmax-retry>Retry imported list</button></div>';if(current(n))host.querySelector('[data-dtmax-retry]').onclick=()=>load(offset);}
    }
    async function facts(next=0){
      const n=++sequence,client=activeClient,requestedKind=kind;factOffset=next;const detail=host.querySelector('[data-dtmax-details]');
      if(!detail)return;detail.innerHTML='<p>Loading imported T1 records…</p>';
      try{
        const p=await request(BASE+'clients/'+encodeURIComponent(client)+'/source-facts?kind='+requestedKind+'&offset='+factOffset+'&limit=25');
        if(!current(n))return;
        if(p.client_number!==client||p.kind!==requestedKind||p.test_copy!==true||p.tax_cases_imported!==0||p.offset!==factOffset||!integer(p.total)||!Array.isArray(p.records)||p.records.length>25)throw Error('Source client response did not match');
        if(p.records.some(r=>requestedKind==='family-files'?!Array.isArray(r.member_client_numbers)||!r.member_client_numbers.includes(client):r.client_number!==client))throw Error('Source record belongs to another client');
        detail.innerHTML='<h3>Imported T1 · Client '+esc(client)+'</h3><div class="row">'+Object.entries(kinds).map(([k,label])=>'<button class="btn sm" type="button" data-source-kind="'+k+'"'+(k===kind?' aria-pressed="true"':'')+'>'+label+'</button>').join('')+'</div>'+recordsHtml(kind,p.records)+'<p>'+ (p.total?factOffset+1:0)+'–'+(factOffset+p.records.length)+' of '+p.total+' source records</p><button class="btn sm" data-fact-prev '+(!factOffset?'disabled':'')+'>Previous records</button> <button class="btn sm" data-fact-next '+(factOffset+p.records.length>=p.total?'disabled':'')+'>Next records</button>';
        detail.querySelectorAll('[data-source-kind]').forEach(b=>b.onclick=()=>{kind=b.dataset.sourceKind;return facts(0);});detail.querySelector('[data-fact-prev]').onclick=()=>facts(Math.max(0,factOffset-25));detail.querySelector('[data-fact-next]').onclick=()=>facts(factOffset+25);
        detail.scrollIntoView?.({block:'nearest'});
      }catch(e){if(current(n))detail.innerHTML='<p role="alert">Imported T1 records unavailable: '+esc(e.message)+'</p>';}
    }
    return {ready:load(),load,facts};
  }
  globalThis.NXTDTMaxT1={mount,cents,validatePage,recordsHtml};
})();
