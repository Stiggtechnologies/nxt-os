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
  function checkHistorical(r){
    if(!r||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||!r.values_cents||!['T1','T3'].includes(r.vendor_module)||!['vendor_static_verified','pending_year_verification','pending_module_verification'].includes(r.mapping_state))throw Error('Invalid source figures');
    if((r.vendor_module==='T3')!==(r.mapping_state==='pending_module_verification'))throw Error('Source module mapping did not match');
    if(r.year_mapping_review!==undefined){
      const v=r.year_mapping_review,pins={2013:'9d7254fabd95f679025e3fc7e441304709f259fd7fbfc2425787a231fa3ed07e',2014:'412666ff325b72b66a7a60a50701bcde019357d9a6db991ac2bccaea85256b54'};
      if(!v||r.vendor_module!=='T1'||r.mapping_state!=='pending_year_verification'||v.tax_year!==r.tax_year||!pins[r.tax_year]||v.package_sha256!==pins[r.tax_year]||v.state!=='original_form_bindings_checked'||v.source_form!=='E11'||v.profile!=='dtmax-T1-E11-2013-2014/v1'||v.amounts_recalculated!==false||v.tax_figures_approved!==false||!v.field_ids||Object.keys(v.field_ids).length!==6||lines.some(l=>v.field_ids[l]!==Number(l)/100))throw Error('Year-specific source mapping review did not match');
    }
    lines.forEach(l=>cents(r.values_cents[l]));return r;
  }
  function mappingLabel(r){return r.year_mapping_review?'Year-specific form bindings checked; source totals':r.mapping_state==='vendor_static_verified'?'Vendor mapping checked; source totals':'Mapping needs verification';}
  function historicalTotalsHtml(record){
    if(!record)return '<p>No imported totals for the selected year.</p>';
    const r=checkHistorical(record);
    if(r.vendor_module!=='T1')return '<h3>Source module '+esc(r.vendor_module)+' · '+r.tax_year+'</h3><p class="dtmax-notice">This source context identifies a different tax module. Its candidate values are not shown as T1 totals. The original source is preserved; separate module mapping is required.</p>';
    return '<h3>Historical totals · '+r.tax_year+'</h3>'+keywordRows(lines.map((l,i)=>[l+' · '+labels[i],cents(r.values_cents[l])]))+'<p>'+esc(mappingLabel(r))+'</p>';
  }
  function recordsHtml(kind,records) {
    if(!records.length)return '<p>No records of this kind are imported for this client.</p>';
    if(kind==='historical-lines')return '<p>Absent means no source value; $0.00 is an explicit zero. Imported totals have not been recalculated.</p><div class="table-wrap"><table><thead><tr><th>Year</th>'+lines.map((l,i)=>'<th>'+labels[i]+' ('+l+')</th>').join('')+'<th>Verification</th></tr></thead><tbody>'+records.map(r=>{
      checkHistorical(r);
      if(r.vendor_module!=='T1')return '<tr><td>'+r.tax_year+'</td><td colspan="6">Source module '+esc(r.vendor_module)+'; T1 totals are unavailable.</td><td>Separate module mapping required</td></tr>';
      return '<tr><td>'+r.tax_year+'</td>'+lines.map(l=>'<td>'+esc(cents(r.values_cents[l]))+'</td>').join('')+'<td>'+esc(mappingLabel(r))+'</td></tr>';
    }).join('')+'</tbody></table></div>';
    if(kind==='family-files')return records.map(r=>'<p>Family file '+esc(r.family_file_id)+' · '+esc(r.tax_year)+'</p><div class="dtmax-family-members">'+(r.member_client_numbers||[]).map(id=>{if(!/^\d{1,12}$/.test(id))throw Error('Invalid family member');return '<button class="dtmax-action" type="button" data-family-client="'+id+'">Client #'+id+'</button>';}).join(' ')+'</div>').join('');
    return '<p>Source occurrences are preserved separately; they are not deduplicated economic slips.</p>'+records.map(r=>'<details><summary>'+esc(r.slip_kind)+' · '+esc(r.tax_year===null?'Missing tax year':r.tax_year)+'</summary><p>Source folder year: '+esc(r.source_folder_year)+' (not an inferred slip year)</p><ul>'+tree(r.vendor_field_tree)+'</ul></details>').join('');
  }
  const missing='<span class="dtmax-missing">Not imported</span>';
  function keywordRows(rows){return '<dl>'+rows.map(([label,value])=>'<dt data-dtmax-keyword="'+esc(label.toLowerCase())+'">'+esc(label)+'</dt><dd>'+(value===undefined||value===null||value===''?missing:esc(value))+'</dd>').join('')+'</dl>';}
  function entryHtml(client,record,comparison){
    const profile=client.source_profile||{};
    const totals=historicalTotalsHtml;
    return '<div class="dtmax-entry"><section class="dtmax-keyword-pane"><h3>Personal information</h3>'+keywordRows([['Client #',client.client_number],['Name',client.name]])+'<details><summary>7 personal fields not imported</summary>'+keywordRows([['Relation',null],['SIN',null],['BirthDate',null],['Status',null],['Sex',null],['Prov-Residence',null],['Language',null]])+'</details>'+totals(record)+'<h3>Mailing address · client-list snapshot</h3>'+keywordRows([['Street',profile.street],['City',profile.city],['Province',profile.province],['PostalCode',profile.postal_code],['Country',profile.country]])+'<h3>Contact · client-list snapshot</h3>'+keywordRows([['Email',profile.email],['Home phone',profile.home_phone],['Work phone',profile.work_phone],['Cell phone',profile.cell_phone]])+'<p>Address, contact and processing status come from the imported client list. They are not established as the historical return’s values.</p></section><section class="dtmax-keyword-pane"><label>Compare historical year <select data-dtmax-compare><option value="">&lt;none&gt;</option>'+(client.historical_years||[]).map(y=>'<option value="'+y+'"'+(String(y)===comparison?' selected':'')+'>'+y+'</option>').join('')+'</select></label><div data-dtmax-comparison></div><h3>Source records</h3><p>Browse the preserved slips and family links below. Source values are read-only.</p><div data-dtmax-details></div></section></div>';
  }
  function validatePdf(r,client){
    if(!r||r.client_number!==client||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||! /^[a-f0-9]{64}$/.test(r.sha256)||r.original_print_output!==true||r.immutable!==true||!integer(r.bytes)||r.bytes===0||r.bytes>2097152||!Array.isArray(r.page_labels)||r.page_labels.length!==r.page_count||r.page_count<1||r.page_count>500||r.page_labels.some(x=>typeof x!=='string'||!x||x.length>120||/[\x00-\x1f]/.test(x)))throw Error('Original PDF identity or page index did not match');return r;
  }
  function validateException(r){
    const digest=s=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s);
    if(!r||!digest(r.source_archive_sha256)||!digest(r.source_context_sha256)||!digest(r.source_year_frame_sha256)||!integer(r.source_context_offset)||!integer(r.source_year_frame_offset)||r.source_year_frame_offset<56||r.source_year_frame_offset+12>r.source_context_offset||!integer(r.source_archive_bytes)||!integer(r.source_context_bytes)||r.source_context_bytes<70||r.source_context_bytes>65536||r.source_context_offset+r.source_context_bytes>r.source_archive_bytes||!/^\d{1,10}$/.test(r.client_number)||!/^\d{1,10}$/.test(r.family_file_id)||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||!['missing_identity_information','empty_information_payload'].includes(r.reason)||r.immutable!==true||r.identity_metadata_readable!==false||r.normalized_amounts_created!==false||r.header_ids_are_source_references_only!==true||!Array.isArray(r.frame_tags)||r.frame_tags.length<2||r.frame_tags.length>256||r.frame_tags[0]!=='EXC'||r.frame_tags[1]!=='EXP'||r.frame_tags.slice(2).some(t=>!['EXR','EXA'].includes(t)))throw Error('Unreadable source context metadata did not match');return r;
  }
  function exceptionsHtml(records){
    records.forEach(validateException);
    if(!records.length)return '<p>No unreadable source contexts are preserved in this scope.</p>';
    return '<p>Original archive fragments are preserved below. Header IDs are source references; they do not establish a person, ownership or tax figures.</p><div class="table-wrap"><table><thead><tr><th>Year</th><th>Header client #</th><th>Family file #</th><th>Source reader exception</th><th>Original bytes</th></tr></thead><tbody>'+records.map((r,i)=>'<tr><td>'+r.tax_year+'</td><td>'+esc(r.client_number)+'</td><td>'+esc(r.family_file_id)+'</td><td>'+esc(r.reason==='missing_identity_information'?'Identity information missing':'Identity information payload empty')+(r.frame_tags.includes('EXA')?'<small>Opaque family auxiliary data included</small>':'')+'</td><td><button type="button" data-exception-download="'+i+'">Download fragment · '+r.source_context_bytes+' bytes</button></td></tr>').join('')+'</tbody></table></div>';
  }
  function mount(host,{request,isCurrent=()=>true}) {
    let sequence=0,offset=0,search='',year='',activeClient='',kind='historical-lines',factOffset=0;
    let page=null,view='list',selectedYear='',comparison='',history=[],pdfs=[],pdfUrl='',pdfRecord=null,pdfPage=1;
    let sort='client_number',direction='asc',status='',province='',exceptionSequence=0;const clientCache=new Map(),exceptionUrls=[];
    const releaseExceptions=()=>{exceptionUrls.splice(0).forEach(url=>URL.revokeObjectURL(url));};
    const current=n=>host.isConnected!==false&&isCurrent()&&n===sequence;
    const q=s=>host.querySelector(s),all=s=>host.querySelectorAll(s);
    const selected=()=>clientCache.get(activeClient);
    const releasePdf=()=>{if(pdfUrl){URL.revokeObjectURL(pdfUrl);pdfUrl='';}pdfRecord=null;};
    function tabs(){return '<div class="dtmax-tabs" role="tablist" aria-label="Imported T1 workspace">'+[['list','Client list','F10'],['entry','Data entry','F2'],['return','Tax return','F9']].map(([id,label,key])=>'<button role="tab" id="dtmax-tab-'+id+'" aria-controls="dtmax-panel" aria-selected="'+(id===view)+'" tabindex="'+(id===view?'0':'-1')+'" data-dtmax-view="'+id+'"'+(id!=='list'&&!activeClient?' disabled':'')+'>'+label+'<kbd>'+key+'</kbd></button>').join('')+'<span class="dtmax-module">T1</span></div>';}
    function shell(body){
      if(view==='list')body='<details data-dtmax-exceptions><summary>Unreadable source contexts</summary><div data-dtmax-exception-content><p>Open to load preserved source exceptions.</p></div></details>'+body;
      host.innerHTML='<section class="dtmax-workspace" aria-label="DT Max source workspace"><div class="dtmax-caption"><h2>T1 clients imported from DT Max</h2><span>Firm-wide source workspace</span></div>'+tabs()+'<p class="dtmax-notice">Preserved source records · Read-only. NXT preparation workpapers remain separate below.</p><div id="dtmax-panel" role="tabpanel" aria-labelledby="dtmax-tab-'+view+'">'+body+'</div></section>';
      all('[data-dtmax-view]').forEach(b=>{b.onclick=()=>show(b.dataset.dtmaxView);b.onkeydown=e=>{if(e.key==='ArrowRight'||e.key==='ArrowLeft'){e.preventDefault();const ids=activeClient?['list','entry','return']:['list'];const at=ids.indexOf(view);return show(ids[(at+(e.key==='ArrowRight'?1:ids.length-1))%ids.length],true);}};});
      if(view==='list')q('[data-dtmax-exceptions]').ontoggle=e=>{if(e.target.open)return loadExceptions(0);};
      host.onkeydown=e=>{
        if(e.ctrlKey&&!e.altKey&&!e.metaKey&&e.key.toLowerCase()==='f'&&view==='list'){e.preventDefault();q('[name=search]')?.focus?.();return;}
        if(e.altKey&&e.key==='F7'&&view==='entry'){e.preventDefault();q('[data-dtmax-keyword-find]')?.focus?.();return;}
        if(e.altKey||e.ctrlKey||e.metaKey||/INPUT|SELECT|TEXTAREA/.test(e.target?.tagName||''))return;const id={F10:'list',F2:'entry',F9:'return'}[e.key];if(id&&(activeClient||id==='list')){e.preventDefault();show(id,true);}
      };
    }
    async function loadExceptions(at){
      const n=sequence,generation=++exceptionSequence,detail=q('[data-dtmax-exception-content]');if(!detail)return;detail.innerHTML='<p>Loading preserved source exceptions…</p>';
      try{
        const p=await request(BASE+'source-exceptions?offset='+at+'&limit=25');if(!current(n)||view!=='list'||generation!==exceptionSequence)return;
        if(p.source_scope!==SCOPE||p.test_copy!==true||p.source_rows_modified!==0||p.tax_cases_imported!==0||p.offset!==at||p.limit!==25||!integer(p.total)||!Array.isArray(p.records)||p.records.length!==Math.min(25,Math.max(0,p.total-at)))throw Error('Source exception response did not match');
        detail.innerHTML=exceptionsHtml(p.records)+'<p>'+(p.total?at+1:0)+'–'+(at+p.records.length)+' of '+p.total+' source contexts</p><button type="button" data-exception-prev '+(!at?'disabled':'')+'>Previous contexts</button> <button type="button" data-exception-next '+(at+p.records.length>=p.total?'disabled':'')+'>Next contexts</button>';
        detail.querySelector('[data-exception-prev]').onclick=()=>loadExceptions(Math.max(0,at-25));detail.querySelector('[data-exception-next]').onclick=()=>loadExceptions(at+25);
        detail.querySelectorAll('[data-exception-download]').forEach(b=>b.onclick=()=>downloadException(p.records[Number(b.dataset.exceptionDownload)],n,generation));
      }catch(e){if(current(n)&&view==='list'&&generation===exceptionSequence)detail.innerHTML='<p role="alert">Source exceptions unavailable: '+esc(e.message)+'</p>';}
    }
    async function downloadException(record,n,generation){
      try{
        const p=await request(BASE+'source-exceptions/'+record.source_archive_sha256+'/'+record.source_context_offset);if(!current(n)||generation!==exceptionSequence)return;
        validateException(p.record);
        if(p.source_scope!==SCOPE||p.test_copy!==true||p.source_rows_modified!==0||p.tax_cases_imported!==0||JSON.stringify(p.record)!==JSON.stringify(record)||typeof p.source_context_hex!=='string'||p.source_context_hex.length!==record.source_context_bytes*2||!/^[a-f0-9]+$/.test(p.source_context_hex))throw Error('Source exception content did not match');
        const bytes=Uint8Array.from(p.source_context_hex.match(/../g),s=>parseInt(s,16));
        const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');if(!current(n)||generation!==exceptionSequence)return;if(digest!==record.source_context_sha256)throw Error('Source fragment failed browser hash verification');
        const url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'}));exceptionUrls.push(url);const link=document.createElement('a');link.href=url;link.download='dtmax-source-context-'+record.tax_year+'-'+record.source_context_sha256.slice(0,12)+'.dtmax-frames';link.click();
      }catch(e){if(current(n)&&generation===exceptionSequence){const detail=q('[data-dtmax-exception-content]');if(detail)detail.innerHTML='<p role="alert">Source fragment unavailable: '+esc(e.message)+'</p>';}}
    }
    function clientHead(){const c=selected();return '<div class="dtmax-client-head">Client #'+esc(c.client_number)+' · '+esc(c.name)+'<small>Imported source · '+(selectedYear?'Tax year '+esc(selectedYear):'No historical year')+'</small></div><div class="dtmax-toolbar"><label>Tax year<select data-dtmax-year>'+c.historical_years.slice().sort((a,b)=>b-a).map(y=>'<option'+(String(y)===selectedYear?' selected':'')+'>'+y+'</option>').join('')+'</select></label><button type="button" data-dtmax-back>Back to client list</button></div>';}
    function bindClient(){q('[data-dtmax-back]').onclick=()=>show('list');q('[data-dtmax-year]').onchange=e=>{selectedYear=e.target.value;show(view);};}
    function renderList(){
      const p=page;
      shell('<form data-dtmax-search class="dtmax-toolbar"><label>Find client<input name="search" type="search" maxlength="100" value="'+esc(search)+'" placeholder="Name or DT Max client number"></label><label>Imported year<select name="year"><option value="">All years</option>'+p.available_years.slice().sort((a,b)=>b-a).map(y=>'<option'+(String(y)===year?' selected':'')+'>'+y+'</option>').join('')+'</select></label><label>Processing status<select name="status"><option value="">All statuses</option>'+(p.available_statuses||[]).map(v=>'<option value="'+esc(v)+'"'+(v===status?' selected':'')+'>'+esc(v)+'</option>').join('')+'</select></label><label>Province<select name="province"><option value="">All provinces</option>'+(p.available_provinces||[]).map(v=>'<option value="'+esc(v)+'"'+(v===province?' selected':'')+'>'+esc(v)+'</option>').join('')+'</select></label><label>Sort by<select name="sort">'+[['client_number','Client number'],['name','Name'],['processing_status','Processing status']].map(([v,label])=>'<option value="'+v+'"'+(v===sort?' selected':'')+'>'+label+'</option>').join('')+'</select></label><label>Order<select name="direction"><option value="asc"'+(direction==='asc'?' selected':'')+'>Ascending</option><option value="desc"'+(direction==='desc'?' selected':'')+'>Descending</option></select></label><button type="submit">Find client</button></form><div class="dtmax-grid"><table><thead><tr><th>Client #</th><th>Name</th><th>Processing status</th><th>Province</th><th>Imported historical years</th></tr></thead><tbody>'+p.clients.map(c=>'<tr data-selected="'+(activeClient===c.client_number)+'"><td>'+esc(c.client_number)+'</td><td><button type="button" class="dtmax-name" data-imported-client="'+esc(c.client_number)+'" aria-label="Open imported T1 for '+esc(c.name)+'">'+esc(c.name)+'</button></td><td>'+esc(c.source_profile?.processing_status||'Not imported')+'</td><td>'+esc(c.source_profile?.province||'Not imported')+'</td><td class="dtmax-years">'+esc(c.historical_years.join(', ')||'No decoded historical totals')+(c.pending_years.length?'<small>Needs verification: '+esc(c.pending_years.join(', '))+'</small>':'')+'</td></tr>').join('')+'</tbody></table>'+(!p.total?'<p>No imported clients match these filters.</p>':'')+'</div><div class="dtmax-footer"><p>'+(p.total?offset+1:0)+'–'+(offset+p.clients.length)+' of '+p.total+' clients</p><div class="dtmax-page-controls"><button type="button" data-dtmax-prev '+(!offset?'disabled':'')+'>Previous</button><button type="button" data-dtmax-next '+(offset+p.clients.length>=p.total?'disabled':'')+'>Next</button></div></div>');
      q('[data-dtmax-search]').onsubmit=e=>{e.preventDefault();search=q('[name=search]').value;year=q('[name=year]').value;sort=q('[name=sort]').value||'client_number';direction=q('[name=direction]').value||'asc';status=q('[name=status]').value;province=q('[name=province]').value;return load(0);};
      q('[data-dtmax-prev]').onclick=()=>load(Math.max(0,offset-100));q('[data-dtmax-next]').onclick=()=>load(offset+100);
      all('[data-imported-client]').forEach(b=>b.onclick=()=>open(b.dataset.importedClient));
    }
    async function load(next=0){
      const n=++sequence;offset=next;activeClient='';view='list';releasePdf();releaseExceptions();
      host.innerHTML='<div class="card"><h2>T1 clients imported from DT Max</h2><p>Loading imported clients…</p></div>';
      try{
        const p=validatePage(await request(BASE+'source-clients?offset='+offset+'&limit=100&search='+encodeURIComponent(search)+(year?'&year='+year:'')+'&sort='+sort+'&direction='+direction+(status?'&status='+encodeURIComponent(status):'')+(province?'&province='+encodeURIComponent(province):'')),offset);
        if(!current(n))return;page=p;clientCache.clear();p.clients.forEach(c=>clientCache.set(c.client_number,c));renderList();
      }catch(e){if(current(n)){host.innerHTML='<div class="dtmax-error"><h2>T1 imported clients unavailable</h2><p role="alert">'+esc(e.message)+'</p><p>Unavailable data is not an empty client list. Firm-wide source access is required.</p><button data-dtmax-retry>Retry imported list</button></div>';q('[data-dtmax-retry]').onclick=()=>load(offset);}}
    }
    async function historyFor(client,n){
      const records=[];let at=0,total;
      do{const p=await request(BASE+'clients/'+client+'/source-facts?kind=historical-lines&offset='+at+'&limit=100');if(!current(n))return null;
        checkFacts(p,client,'historical-lines',at,100);if(total!==undefined&&p.total!==total)throw Error('Source history changed while loading');total=p.total;if(total>1000)throw Error('Source history exceeds view bound');records.push(...p.records);at+=p.records.length;if(at<total&&!p.records.length)throw Error('Incomplete source history page');
      }while(at<total);recordsHtml('historical-lines',records);return records;
    }
    async function open(client){
      if(!/^\d{1,12}$/.test(client))return;const lookup=++sequence;
      if(!clientCache.has(client)){
        try{const found=validatePage(await request(BASE+'source-clients?offset=0&limit=100&search='+encodeURIComponent(client)),0);if(!current(lookup))return;const exact=found.clients.find(c=>c.client_number===client);if(!exact)throw Error('Family member is not in the imported client roster');clientCache.set(client,exact);}
        catch(e){if(current(lookup)){const d=q('[data-dtmax-details]');if(d)d.innerHTML='<p role="alert">'+esc(e.message)+'</p>';}return;}
      }
      activeClient=client;kind='historical-lines';comparison='';history=[];pdfs=[];releasePdf();
      selectedYear=year||String(Math.max(...selected().historical_years)||'');if(selectedYear==='-Infinity')selectedYear='';
      const n=++sequence;view='entry';shell(clientHead()+'<p class="dtmax-notice">Loading source history…</p>');bindClient();
      try{const records=await historyFor(client,n);if(!records||!current(n))return;history=records;return show('entry');}catch(e){if(current(n))shell(clientHead()+'<p class="dtmax-error" role="alert">'+esc(e.message)+'</p>');}
    }
    function historicalRecord(y){const matches=history.filter(r=>String(r.tax_year)===y);return matches.length===1?matches[0]:null;}
    async function show(next,focus=false){
      if(next!=='list'&&!selected())return;const n=++sequence;view=next;
      if(next==='list'){releasePdf();renderList();}
      else if(next==='entry'){
        releasePdf();shell(clientHead()+'<div class="dtmax-toolbar"><label>Find keyword <input type="search" data-dtmax-keyword-find placeholder="Keyword or line number · Alt+F7"></label><span class="dtmax-missing" data-dtmax-keyword-count></span></div>'+entryHtml(selected(),historicalRecord(selectedYear),comparison));bindClient();
        q('[data-dtmax-keyword-find]').oninput=e=>{const needle=e.target.value.trim().toLowerCase();let count=0;all('[data-dtmax-keyword]').forEach(dt=>{const match=!needle||dt.dataset.dtmaxKeyword.includes(needle);dt.hidden=!match;if(dt.nextElementSibling)dt.nextElementSibling.hidden=!match;if(match)count++;});q('[data-dtmax-keyword-count]').textContent=needle?count+' matching fields':'';};
        if(history.filter(r=>String(r.tax_year)===selectedYear).length>1)q('[data-dtmax-comparison]').innerHTML='<p role="alert">Multiple source versions exist for this year. Review Historical totals below; no version was selected automatically.</p>';
        q('[data-dtmax-compare]').onchange=e=>{comparison=e.target.value;renderComparison();};renderComparison();await facts(0,n);
      }else{
        shell(clientHead()+'<div class="dtmax-return"><aside class="dtmax-page-index"><h3>Select a page</h3><div data-dtmax-pdf-index>Loading original PDFs…</div></aside><div class="dtmax-pdf-canvas" data-dtmax-pdf-canvas><div class="dtmax-pdf-message">Loading preserved print output…</div></div></div>');bindClient();
        try{const p=await request(BASE+'clients/'+activeClient+'/return-pdfs');if(!current(n))return;if(p.source_scope!==SCOPE||p.client_number!==activeClient||p.test_copy!==true||p.tax_cases_imported!==0||!Array.isArray(p.records)||p.records.length>1000)throw Error('Original PDF list did not match');pdfs=p.records.map(r=>validatePdf(r,activeClient));renderPdfIndex();const versions=pdfs.filter(r=>String(r.tax_year)===selectedYear);const r=versions.length===1?versions[0]:null;if(r)await openPdf(r,n);else{releasePdf();pdfMissing();}}catch(e){if(current(n)){releasePdf();q('[data-dtmax-pdf-index]').innerHTML='Unavailable';q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message" role="alert">Original PDF unavailable: '+esc(e.message)+'</div>';}}
      }
      if(focus&&current(sequence))q('[data-dtmax-view="'+view+'"]')?.focus?.();
    }
    function renderComparison(){const node=q('[data-dtmax-comparison]');if(!node)return;const r=historicalRecord(comparison);node.innerHTML=comparison?(r?historicalTotalsHtml(r):'<p>No single imported source version for this comparison year.</p>'):'<p class="dtmax-missing">Choose a year to compare the six preserved totals.</p>';}
    function checkFacts(p,client,requestedKind,at,limit){
      if(!p||p.client_number!==client||p.kind!==requestedKind||p.test_copy!==true||p.tax_cases_imported!==0||p.offset!==at||!integer(p.total)||!Array.isArray(p.records)||p.records.length>limit||at+p.records.length>p.total)throw Error('Source client response did not match');
      if(p.records.some(r=>requestedKind==='family-files'?!Array.isArray(r.member_client_numbers)||!r.member_client_numbers.includes(client):r.client_number!==client))throw Error('Source record belongs to another client');
    }
    async function facts(next=0,existingSequence){
      const n=existingSequence??++sequence,client=activeClient,requestedKind=kind;factOffset=next;const detail=q('[data-dtmax-details]');if(!detail)return;detail.innerHTML='<p>Loading imported T1 records…</p>';
      try{
        const p=await request(BASE+'clients/'+client+'/source-facts?kind='+requestedKind+'&offset='+next+'&limit=25');if(!current(n))return;checkFacts(p,client,requestedKind,next,25);
        detail.innerHTML='<div class="dtmax-fact-nav">'+Object.entries(kinds).map(([k,label])=>'<button type="button" data-source-kind="'+k+'" aria-pressed="'+(k===kind)+'">'+label+'</button>').join('')+'</div>'+recordsHtml(kind,p.records)+'<p>'+(p.total?next+1:0)+'–'+(next+p.records.length)+' of '+p.total+' source records</p><button class="dtmax-action" data-fact-prev '+(!next?'disabled':'')+'>Previous records</button> <button class="dtmax-action" data-fact-next '+(next+p.records.length>=p.total?'disabled':'')+'>Next records</button>';
        detail.querySelectorAll('[data-source-kind]').forEach(b=>b.onclick=()=>{kind=b.dataset.sourceKind;return facts(0);});detail.querySelector('[data-fact-prev]').onclick=()=>facts(Math.max(0,factOffset-25));detail.querySelector('[data-fact-next]').onclick=()=>facts(factOffset+25);
        detail.querySelectorAll('[data-family-client]').forEach(b=>b.onclick=()=>open(b.dataset.familyClient));
      }catch(e){if(current(n))detail.innerHTML='<p role="alert">Imported T1 records unavailable: '+esc(e.message)+'</p>';}
    }
    function pdfMissing(){q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message"><h3>Original printed return not imported for '+esc(selectedYear||'this year')+'</h3><p>The historical totals and slips are available in Data entry. A complete printed return needs the original DT Max PDF for this client and year.</p><p>No return pages have been reconstructed from incomplete figures.</p></div>';}
    function renderPdfIndex(){
      const node=q('[data-dtmax-pdf-index]');node.innerHTML='<label>Original PDF<select data-dtmax-pdf-version><option value="">Choose original PDF</option>'+pdfs.map((r,i)=>'<option value="'+i+'"'+(pdfRecord?.sha256===r.sha256&&pdfRecord?.tax_year===r.tax_year?' selected':'')+'>'+r.tax_year+' · '+r.page_count+' pages · Version '+(i+1)+'</option>').join('')+'</select></label>'+(pdfRecord?pdfRecord.page_labels.map((label,i)=>'<button type="button" data-dtmax-page="'+(i+1)+'" aria-current="'+(pdfPage===i+1?'page':'false')+'">'+(i+1)+'. '+esc(label)+'</button>').join(''):'<p class="dtmax-missing">'+(pdfs.length?'Choose an available original above.':'No original printed returns imported for this client.')+'</p>');
      q('[data-dtmax-pdf-version]').onchange=e=>{const r=pdfs[Number(e.target.value)];if(e.target.value!==''&&r){selectedYear=String(r.tax_year);return openPdf(r,++sequence);}};
      all('[data-dtmax-page]').forEach(b=>b.onclick=()=>{pdfPage=Number(b.dataset.dtmaxPage);renderPdfCanvas();renderPdfIndex();});
    }
    async function openPdf(record,n){
      const client=activeClient;releasePdf();q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message">Verifying original PDF bytes…</div>';
      try{const p=await request(BASE+'clients/'+client+'/return-pdfs/'+record.tax_year+'/'+record.sha256);if(!current(n))return;const r=validatePdf(p.record,client);
        if(p.source_scope!==SCOPE||p.test_copy!==true||p.tax_cases_imported!==0||r.sha256!==record.sha256||r.tax_year!==record.tax_year||JSON.stringify(r.page_labels)!==JSON.stringify(record.page_labels)||typeof p.content_hex!=='string'||p.content_hex.length!==r.bytes*2||!/^[0-9a-f]+$/.test(p.content_hex))throw Error('Original PDF content did not match');
        const bytes=Uint8Array.from(p.content_hex.match(/../g),s=>parseInt(s,16));const actual=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');if(!current(n))return;if(actual!==r.sha256)throw Error('Original PDF failed browser hash verification');
        pdfUrl=URL.createObjectURL(new Blob([bytes],{type:'application/pdf'}));pdfRecord=r;pdfPage=1;renderPdfIndex();renderPdfCanvas();
      }catch(e){if(current(n)){releasePdf();q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message" role="alert">Original PDF unavailable: '+esc(e.message)+'</div>';}}
    }
    function renderPdfCanvas(){
      if(!pdfUrl||!pdfRecord)return;const title=pdfRecord.page_labels[pdfPage-1];q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-tools"><p>'+esc(pdfRecord.tax_year)+' · Page '+pdfPage+' of '+pdfRecord.page_count+' · Original bytes verified</p><a href="'+esc(pdfUrl)+'" download="dtmax-t1-'+esc(activeClient)+'-'+pdfRecord.tax_year+'.pdf">Download original PDF</a></div><iframe title="Original DT Max printed return: '+esc(title)+'" referrerpolicy="no-referrer" src="'+esc(pdfUrl)+'#page='+pdfPage+'&view=FitH&navpanes=0"></iframe>';
    }
    return {ready:load(),load,facts,show,open,dispose:()=>{++sequence;releasePdf();releaseExceptions();host.onkeydown=null;}};
  }
  globalThis.NXTDTMaxT1={mount,cents,validatePage,recordsHtml,entryHtml,validatePdf,validateException,exceptionsHtml};
})();
