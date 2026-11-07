/* Read-only imported personal T1 source records. Never creates/calculates a tax case. */
(function () {
  'use strict';
  const SCOPE='dtmax-test-copy', BASE='/api/source-imports/dtmax/companies/'+SCOPE+'/test-copy/';
  const esc=v=>String(v??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
  const lines=['15000','23600','26000','42000','48400','48500'];
  const labels=['Total income','Net income','Taxable income','Net federal tax','Refund','Balance owing'];
  const kinds={'historical-lines':'Historical totals','historical-variants':'Source versions','printed-cached-figures':'Printed cached figures','afr-slips':'CRA slips','native-slip-views':'Native slip fields','linked-t5008':'Linked T5008 CSV','family-files':'Family files','historical-family-lineage':'Recovered family links'};
  const printedFigureProfile='502e76e71cc132a5f74154c364ed398e0eae5cbb8c595a2caceff934b2ae0bf6';
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
      const v=r.year_mapping_review,pins={2005:'90e36c34c9ad98691c872b6d0bc5482459ec5db84f9f32f86b926f234234f745',2009:'78451f0ea751567f4c34ce308bbca508c9f8a6dc0f13697249e5e008b7a4229f',2010:'d51d03f137a7b9c7a37e91bd8aeb5655e887fda56dfb800254829bb125092082',2011:'fc934ed6c99be48e65729cc1f4187e49eb15a66f7010a64ddafab52e7007e6b0',2012:'8ce020c728d0cf6b9ea8804c368dd281f7732d2b03103d6f8f69c68efd9013ce',2013:'9d7254fabd95f679025e3fc7e441304709f259fd7fbfc2425787a231fa3ed07e',2014:'412666ff325b72b66a7a60a50701bcde019357d9a6db991ac2bccaea85256b54',2026:'0f20e028ba2dbf059f6d585c16c56f4243c2cdbe332a7e1b1ef8de14985ccbb5'};
      const profile=r.tax_year===2005?'dtmax-T1-E11-2005/v1':[2013,2014].includes(r.tax_year)?'dtmax-T1-E11-2013-2014/v1':'dtmax-T1-E11-additional-years/v1';
      if(!v||r.vendor_module!=='T1'||r.mapping_state!=='pending_year_verification'||v.tax_year!==r.tax_year||!pins[r.tax_year]||v.package_sha256!==pins[r.tax_year]||v.state!=='original_form_bindings_checked'||v.source_form!=='E11'||v.profile!==profile||v.amounts_recalculated!==false||v.tax_figures_approved!==false||(![2013,2014].includes(r.tax_year)&&v.source_year_may_be_preliminary!==(r.tax_year===2026))||!v.field_ids||Object.keys(v.field_ids).length!==6||lines.some(l=>v.field_ids[l]!==Number(l)/100))throw Error('Year-specific source mapping review did not match');
    }
    lines.forEach(l=>cents(r.values_cents[l]));nativeModuleReviewed(r);return r;
  }
  function nativeModuleReviewed(r){
    const p=r.native_module_review;if(p===undefined)return false;
    const hash=s=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s);
    if(!p||p.schema!=='dtmax-native-source-module-review/v1'||p.binding_profile_sha256!=='d33fd86f7924d50e96f6ce4916ff14df8d4740a46863fb13cb1900842dd59604'||p.client_number!==r.client_number||p.family_file_id!==r.family_file_id||p.source_year!==r.tax_year||p.archive_sha256!==r.archive_sha256||r.vendor_module!=='T3'||r.mapping_state!=='pending_module_verification'||p.reader_module!=='T3'||p.native_source_module!=='T1'||p.review_state!=='native_source_module_origin_verified'||['original_history_updated','personal_sidecar_flags_updated','identity_verified','tax_figures_approved'].some(k=>p[k]!==false)||p.native_authority?.module_index!==1||p.native_authority.path_rule_exact!==true||p.native_authority.source_archive_magic!=='0xaeaeaebe'||p.native_file?.relative_path!=='T1\\'+r.tax_year+'\\'+r.family_file_id+'.P'+String(r.tax_year).slice(-2)||p.native_file.physical_raw_stream!==true||!hash(p.native_file.sha256)||p.info1?.crc_verified!==true||p.e11?.source_personal_prefix!==182||!p.e11.fields||Object.keys(p.e11.fields).length!==6)throw Error('Native T1 source-origin review did not match');
    const personal=r.source_personal_info,frames=p.source_frames;
    if(!personal||personal.binding_profile_sha256!=='9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e'||personal.client_number!==r.client_number||personal.family_file_id!==r.family_file_id||personal.source_year!==r.tax_year||personal.archive_sha256!==r.archive_sha256||personal.reader_module!=='T3'||personal.t1_display_eligible!==false||personal.display_state!=='held_module_origin'||!personal.info1||!frames?.EXF||!frames.EXY||!frames.EXC||['frame_offset','frame_sha256','expanded_sha256','expanded_bytes','crc_verified'].some(k=>personal.info1[k]!==p.info1[k])||![p.info1.frame_sha256,p.info1.expanded_sha256,frames.EXC.frame_sha256].every(hash)||personal.source_EXF_file_id!==frames.EXF.selected_file_id||personal.source_year_frame_offset!==frames.EXY.frame_offset||personal.source_context_offset!==frames.EXC.frame_offset||personal.source_EXC_sha256!==frames.EXC.frame_sha256||frames.EXC.client_number!==r.client_number||frames.EXY.native_tax_year!==r.tax_year||frames.EXF.selected_file_id!==frames.EXC.member_file_id||String(frames.EXF.selected_file_id)!==r.family_file_id||frames.EXC.descriptor_terminal_byte!==0)throw Error('Native T1 source review and personal frame anchors differ');
    if(typeof p.e11.reader_has_calculated_return!=='boolean'||p.e11.reader_has_calculated_return!==(p.e11.form!==null))throw Error('Native calculated-source presence did not match');
    for(const line of lines){
      const f=p.e11.fields[line];
      if(!f||f.field_id!==Number(line)/100)throw Error('Native T1 source field did not match');
      if(f.state==='absent'){if(r.values_cents[line]!==null||f.source_wire_integer!==null||f.source_decimal!==null||f.scale_divisor!==null)throw Error('Absent native source figure changed');}
      else if(['explicit_zero','present_value'].includes(f.state)){
        if(!Number.isInteger(f.source_wire_integer)||f.source_wire_integer< -2147483648||f.source_wire_integer>2147483647||f.scale_divisor!==100||typeof r.values_cents[line]!=='string'||!/^(-?[1-9]\d*|0)$/.test(r.values_cents[line])||BigInt(f.source_wire_integer)!==BigInt(r.values_cents[line])||(f.state==='explicit_zero')!==(f.source_wire_integer===0)||p.e11.form?.name!=='E11'||p.e11.form.record_type!==30||p.e11.form.outer_plan!==0||p.e11.form.crc_verified!==true||p.e11.form.marker_field1_present!==true)throw Error('Native T1 source figure differs from preserved cents');
        const n=BigInt(f.source_wire_integer),a=n<0n?-n:n,fraction=a%100n,decimal=(n<0n?'-':'')+(a/100n)+(fraction?'.'+fraction.toString().padStart(2,'0').replace(/0$/,''):'');if(f.source_decimal!==decimal)throw Error('Native source decimal differs from its wire value');
      }else throw Error('Unknown native source figure state');
    }
    return true;
  }
  function mappingLabel(r){return nativeModuleReviewed(r)?'Native T1 origin and original E11 fields verified; original converter label T3 retained':r.year_mapping_review?'Year-specific form bindings checked; source totals'+(r.tax_year===2026?' — may be preliminary or planning data':''):r.mapping_state==='vendor_static_verified'?'Vendor mapping checked; source totals':'Mapping needs verification';}
  function historicalTotalsHtml(record){
    if(!record)return '<p>No imported totals for the selected year.</p>';
    const r=checkHistorical(record);
    if(r.vendor_module!=='T1'&&!nativeModuleReviewed(r))return '<h3>Converter label '+esc(r.vendor_module)+' · '+r.tax_year+'</h3><p class="dtmax-notice">The converter inferred this module label. Its candidate values are not shown as T1 totals while the original source module is verified. The original source is preserved.</p>';
    return '<h3>Historical totals · '+r.tax_year+'</h3>'+keywordRows(lines.map((l,i)=>[l+' · '+labels[i],cents(r.values_cents[l])]))+'<p>'+esc(mappingLabel(r))+'</p><p>Reader-selected figures. Compare retained Production and federal snapshot figures under Source versions.</p>';
  }
  function nativeSlipViewsHtml(records) {
    const hash=v=>typeof v==='string'&&/^[0-9a-f]{64}$/.test(v),profile='4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc';
    return '<p>Raw financial source fields. Source return year is not an inferred slip year. Allocations and duplicate removal have not been applied.</p>'+records.map(r=>{
      if(!r||!/^\d{1,12}$/.test(r.client_number)||!/^\d{1,12}$/.test(r.family_file_id)||!Number.isInteger(r.source_return_year)||r.source_return_year<1990||r.source_return_year>2100||!['T1','T3'].includes(r.vendor_module)||typeof r.vendor_slip_kind!=='string'||!integer(r.vendor_slip_ordinal)||!integer(r.source_view_number)||r.source_view_number===0||typeof r.from_data_segment!=='boolean'||![r.source_archive_sha256,r.source_view_file_sha256,r.source_view_line_sha256].every(hash)||r.financial_profile_sha256!==profile||!r.field_labels||!Array.isArray(r.financial_fields)||!integer(r.original_field_count)||r.financial_fields.length>512||r.financial_fields.length>r.original_field_count||r.economic_slip_uniqueness_verified!==false||r.normalized_tax_slip_created!==false||r.taxpayer_allocations_applied!==false||r.financial_values_approved!==false)throw Error('Native slip source profile did not match');
      const extra=r.additional_financial_fields??[];
      if(!Array.isArray(extra)||extra.length>512||extra.length&&(r.supplemental_profile_sha256!=='17b49048f15e6bac3b0feee2aad55b4b9879716eadb34805c4ad00e8ec06667d'||r.vendor_module!=='T1'||r.source_return_year<2015||r.source_return_year>2024))throw Error('Additional source profile did not match');
      if(r.financial_fields.length+extra.length>r.original_field_count||r.financial_fields.some((f,i)=>!f||i>0&&r.financial_fields[i-1].source_field_index>=f.source_field_index))throw Error('Original source field order did not match');
      let extraPrevious=-1;
      for(const f of extra){if(!f||!integer(f.source_field_index)||f.source_field_index<=extraPrevious||r.financial_fields.some(core=>core.source_field_index===f.source_field_index||core.field_id===f.field_id)||f.source_type_tag!==4||f.value?.kind!=='text'||typeof f.value.text!=='string'||!f.value.text.trim())throw Error('Additional source fields overlap or have an inactive value');extraPrevious=f.source_field_index;}
      function reviewedLabel(review,f){
        if(review===undefined)return null;
        if(review?.authority_matrix_sha256!=='e20addca37d6105dfa1fb45b12c136a436c40d40d7bd0b27a94101a9e298345d'||review.source_return_year!==r.source_return_year||review.box_assignment_approved!==false||review.normalization_approved!==false||!review.field_bindings)throw Error('Historical source review did not match');
        if(Object.keys(review.field_bindings).length&&(r.vendor_module!=='T1'||r.source_return_year<2015||r.source_return_year>2024))throw Error('Historical source review is outside its verified years');
        const b=review.field_bindings[f.field_id];if(b===undefined)return null;
        if(!Array.isArray(b.possible_labels)||!b.possible_labels.length||b.possible_labels.some(s=>typeof s!=='string'||s.length>1000)||b.selector_review_required!==true||b.box_assignment_approved!==false||b.normalization_approved!==false)throw Error('Historical source label did not match');
        return b.possible_labels.join(' / ')+' · Conversion selection needs review';
      }
      let previous=-1;
      const body=[...r.financial_fields,...extra].sort((a,b)=>a.source_field_index-b.source_field_index).map(f=>{
        const additional=extra.includes(f);
        if(!f||!integer(f.field_id)||!integer(f.source_field_index)||f.source_field_index<=previous||f.source_field_index>=r.original_field_count||!Number.isInteger(f.source_type_tag)||f.source_type_tag<0||f.source_type_tag>255||!additional&&typeof r.field_labels[f.field_id]!=='string'||!f.value)throw Error('Native financial field order or profile did not match');
        previous=f.source_field_index;const v=f.value;let display;
        if(v.kind==='text'&&![2,6,8,11].includes(f.source_type_tag)&&(v.text===null||typeof v.text==='string'&&v.text.length<=4096))display=v.text===null||!v.text.trim()?'<span class="dtmax-missing">Present, blank</span>':'<span class="dtmax-source-value">'+esc(v.text)+'</span>';
        else if(v.kind==='integer'&&[8,11].includes(f.source_type_tag)&&Number.isInteger(v.value)&&v.value>=-2147483648&&v.value<=2147483647&&(f.source_type_tag!==8||v.value>=-32768&&v.value<=32767))display=esc(v.value);
        else if(v.kind==='flag'&&f.source_type_tag===6&&[0,1].includes(v.value))display='Flag '+v.value;
        else if(v.kind==='float32'&&f.source_type_tag===2&&/^[0-9a-f]{8}$/.test(v.bits_hex))display='<span class="dtmax-missing">Numeric format review needed</span>';
        else if(v.kind==='held'&&hash(v.source_field_sha256))display='<span class="dtmax-missing">Unparsed; original source retained</span>';
        else throw Error('Native value does not match its active type');
        const yearLabel=reviewedLabel(additional?r.additional_field_review:r.historical_field_review,f);
        if(additional&&!yearLabel)throw Error('Additional source field lacks its year-specific review');
        const label=r.source_return_year===2025&&r.vendor_module==='T1'?r.field_labels[f.field_id]:yearLabel??'Historical box binding pending';
        return '<tr><th scope="row">'+esc(label)+'<small>Source field '+f.field_id+(additional?' · Additional recovered field':'')+'</small></th><td>'+display+'</td></tr>';
      }).join('');
      const pending=(r.vendor_module!=='T1'?'Converter module label needs verification.':r.source_return_year!==2025?'Historical year bindings need review.':'Source-2025 financial field profile checked; box conversion remains pending.')+(r.source_return_year===2026?' May be preliminary or planning source data.':'');
      return '<details class="dtmax-native-slip"><summary>'+esc(r.vendor_slip_kind)+' · Source return year '+r.source_return_year+' · View '+(r.vendor_slip_ordinal+1)+' · '+(r.from_data_segment?'Data source':'Cached source')+'</summary><p>'+esc(pending)+'</p>'+(body?'<div class="table-wrap"><table><thead><tr><th>Source field</th><th>Active source value</th></tr></thead><tbody>'+body+'</tbody></table></div>':'<p>No fields from this financial profile are present in this view. Other original fields remain in the preserved extract.</p>')+mergedSourceHtml(r)+'<details><summary>Source selection and provenance</summary><p>'+esc(r.selection_review)+'</p><p>Archive '+esc(r.source_archive_sha256)+'<br>View file '+esc(r.source_view_file_sha256)+'<br>Original view '+r.source_view_number+' · '+esc(r.source_view_line_sha256)+'</p></details></details>';
    }).join('');
  }
  function mergedSourceHtml(r){
    const s=r.merged_component_financial_source,v=r.merged_component_field_review;
    if(s===undefined&&v===undefined)return '';
    const pin='bd98c9f6157e9cd3cb26befa00957c80a639575ba9bb987c45947fce8ae28358',hash=x=>typeof x==='string'&&/^[a-f0-9]{64}$/.test(x);
    if(!s||!v||s.schema!=='dtmax-native-merged-component-financial-sidecar/v1'||s.merged_component_profile_sha256!==pin||v.profile_sha256!==pin||v.source_fields_only!==true||['box_assignment_approved','normalization_approved','aggregation_approved','allocation_approved','financial_values_approved'].some(k=>v[k]!==false)||['client_number','family_file_id','source_return_year','vendor_module','vendor_slip_kind','vendor_slip_ordinal','vendor_form','source_view_number','source_view_file_sha256','source_view_line_sha256','source_archive_sha256','original_field_count','from_data_segment'].some(k=>s[k]!==r[k])||r.vendor_module!=='T1'||r.vendor_slip_kind!=='T5008'||r.vendor_form!=='92'||r.source_return_year<2015||r.source_return_year>2018||r.from_data_segment!==true||s.primary_view!=='vendor_GetT1TaxSlip_preferred_view'||![s.raw_forms_file_sha256,s.raw_forms_line_sha256].every(hash)||!integer(s.raw_forms_line_number)||s.raw_forms_line_number<1||!Array.isArray(s.source_components)||s.source_components.length!==1||!Array.isArray(s.financial_fields)||!s.financial_fields.length||s.financial_fields.length>512)throw Error('Merged financial source context did not match');
    const c=s.source_components[0];
    if(!c||c.source_component_index!==1||c.form_name!=='171'||c.matching_form_occurrence!==r.vendor_slip_ordinal||![0,7].includes(c.q_plan)||typeof c.from_data_segment!=='boolean'||c.selector_keyword_id!==null||c.selector_option_number!==null||!integer(c.original_field_count)||!c.original_field_count||!hash(c.source_component_sha256))throw Error('Merged source component did not match');
    let previous=-1;
    const body=s.financial_fields.map(f=>{
      if(!f||![14485,14486,14488].includes(f.field_id)||f.source_component_index!==1||!integer(f.source_form_field_index)||f.source_form_field_index<=previous||f.source_form_field_index>=c.original_field_count||f.merged_field_index!==r.original_field_count+f.source_form_field_index||f.source_type_tag!==4||f.value?.kind!=='text'||typeof f.value.text!=='string'||!f.value.text.trim()||f.value.text.length>4096||!hash(f.source_field_sha256)||!Array.isArray(f.selector_references)||!f.selector_references.length||f.selector_references.some(x=>!integer(x.binding_ordinal)||!hash(x.selector_binding_sha256)||x.source_call?.accessor!=='M'))throw Error('Merged financial field provenance did not match');
      previous=f.source_form_field_index;
      return '<tr><th scope="row">Source field '+f.field_id+'<small>Recovered component 171</small></th><td><span class="dtmax-source-value">'+esc(f.value.text)+'</span></td></tr>';
    }).join('');
    return '<h4>Additional T5008 source figures</h4><p>Recovered from the component selected by DT Max’s converter for this slip. Values remain separate source fields; tax-box assignment needs review.</p><div class="table-wrap"><table><thead><tr><th>Source field</th><th>Active source value</th></tr></thead><tbody>'+body+'</tbody></table></div>';
  }
  function recordsHtml(kind,records) {
    if(!records.length)return '<p>No records of this kind are imported for this client.</p>';
    if(kind==='native-slip-views')return nativeSlipViewsHtml(records);
    if(kind==='historical-variants')return historicalVariantsHtml(records);
    if(kind==='printed-cached-figures')return printedCachedFiguresHtml(records);
    if(kind==='historical-family-lineage')return '<p>Historical family-file associations recovered from preserved native indexes and corroborated source identities. Original file IDs are retained. These files have not been decoded into tax figures.</p>'+records.map(r=>{
      const hash=s=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s);
      if(!r||r.schema!=='dtmax-historical-native-family-lineage/v1'||r.lineage_profile_sha256!=='f05734d0cd42f21a347f3dbbdba428cb9b25e0d62ef3b18b5db9c287b855edb0'||r.source_ids_rewritten!==false||r.tax_payloads_decoded!==false||r.tax_values_approved!==false||!/^[0-9]{1,12}$/.test(r.source_family_file_id)||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||![r.source_file_sha256,r.legacy_record_sha256,r.authority_proof_sha256].every(hash)||r.legacy_record_id!==r.source_family_file_id+':'+r.tax_year||!Array.isArray(r.member_mappings)||!r.member_mappings.length||r.member_mappings.length>128)throw Error('Historical family lineage did not match');
      return '<details><summary>Family file '+esc(r.source_family_file_id)+' · '+r.tax_year+'</summary><div class="dtmax-family-members">'+r.member_mappings.map(m=>{if(!m||!/^[0-9]{1,12}$/.test(m.old_native_client_number)||!/^[0-9]{1,12}$/.test(m.current_client_number)||!hash(m.native_name_equality_evidence_sha256)||!hash(m.native_DOB_equality_evidence_sha256))throw Error('Historical family member evidence did not match');return '<p>Historical client #'+esc(m.old_native_client_number)+' → <button class="dtmax-action" type="button" data-family-client="'+m.current_client_number+'">Client #'+m.current_client_number+'</button></p>';}).join('')+'</div><p>Original source file '+esc(r.source_file_sha256)+'</p></details>';
    }).join('');
    if(kind==='linked-t5008')return '<p>Original CSV rows linked by recipient name and matching stored CRA records. The same CRA evidence is available under CRA slips. No additional tax slip has been created.</p>'+records.map(r=>{
      const s=r?.original_source_record,l=r?.owner_link;
      if(!s||!l||!/^\d{1,12}$/.test(r.client_number)||l.owner_client_number!==r.client_number||r.tax_year!==s.tax_year||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||l.legacy_record_id!==[s.client_number,s.tax_year,s.source_file_sha256,s.row].join(':')||! /^[a-f0-9]{64}$/.test(l.legacy_record_sha256)||!['recipient_name_and_all_boxes','same_recipient_file_and_all_boxes'].includes(l.evidence_mode)||!Array.isArray(l.matching_cra_records)||!l.matching_cra_records.length||r.source_owner_id_changed!==false||r.normalized_tax_slip_created!==false||r.economic_slip_uniqueness_verified!==false||r.financial_values_approved!==false||!s.boxes||Object.entries(s.boxes).some(([b,v])=>!['11','13','15','16','17','18','19','20','21','22','23','24'].includes(b)||typeof v!=='string'||v.length>4096))throw Error('Linked CSV source or owner did not match');
      return '<details><summary>T5008 CSV · '+r.tax_year+' · Source row '+esc(s.row)+'</summary><dl>'+Object.entries(s.boxes).map(([b,v])=>'<dt>Box '+b+'</dt><dd>'+esc(v||'Blank in source')+'</dd>').join('')+'</dl><p>Original source client reference: '+esc(s.client_number)+' · '+l.matching_cra_records.length+' matching CRA source occurrence(s). '+(l.evidence_mode==='same_recipient_file_and_all_boxes'?'Other verified rows in the same recipient file also corroborate this link.':'Recipient name and every populated source box are corroborated.')+'</p></details>';
    }).join('');
    if(kind==='historical-lines')return '<p>Absent means no source value; $0.00 is an explicit zero. Imported totals have not been recalculated.</p><div class="table-wrap"><table><thead><tr><th>Year</th>'+lines.map((l,i)=>'<th>'+labels[i]+' ('+l+')</th>').join('')+'<th>Verification</th></tr></thead><tbody>'+records.map(r=>{
      checkHistorical(r);
      if(r.vendor_module!=='T1'&&!nativeModuleReviewed(r))return '<tr><td>'+r.tax_year+'</td><td colspan="6">Converter label '+esc(r.vendor_module)+'; T1 totals are unavailable.</td><td>Source module verification required</td></tr>';
      return '<tr><td>'+r.tax_year+'</td>'+lines.map(l=>'<td>'+esc(cents(r.values_cents[l]))+'</td>').join('')+'<td>'+esc(mappingLabel(r))+'</td></tr>';
    }).join('')+'</tbody></table></div>';
    if(kind==='family-files')return records.map(r=>'<p>Family file '+esc(r.family_file_id)+' · '+esc(r.tax_year)+'</p><div class="dtmax-family-members">'+(r.member_client_numbers||[]).map(id=>{if(!/^\d{1,12}$/.test(id))throw Error('Invalid family member');return '<button class="dtmax-action" type="button" data-family-client="'+id+'">Client #'+id+'</button>';}).join(' ')+'</div>').join('');
    return '<p>Source occurrences are preserved separately; they are not deduplicated economic slips.</p>'+records.map(r=>'<details><summary>'+esc(r.slip_kind)+' · '+esc(r.tax_year===null?'Missing tax year':r.tax_year)+'</summary><p>Source folder year: '+esc(r.source_folder_year)+' (not an inferred slip year)</p><ul>'+tree(r.vendor_field_tree)+'</ul></details>').join('');
  }
  function historicalVariantsHtml(records){
    const pin='641c2b5f56617882829d5049813e06789a3a661580fe6f6d3adf6f8dde5e03ba',hash=x=>typeof x==='string'&&/^[0-9a-f]{64}$/.test(x);
    return '<p>Stored calculation versions may differ. Every retained E11 form and repeated field is shown separately. No version has been selected as authoritative or recalculated.</p>'+records.map(r=>{
      if(!r||r.schema!=='dtmax-native-E11-source-variants/v1'||r.source_variant_profile_sha256!==pin||r.source_archive_sha256!=='499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c'||!/^\d{1,12}$/.test(r.client_number)||!/^\d{1,12}$/.test(r.family_file_id)||!Number.isInteger(r.source_return_year)||r.source_return_year<1990||r.source_return_year>2100||!['T1','T3'].includes(r.reader_module)||r.reader_module==='T3'&&r.native_T1_origin_guard_verified!==true||r.legacy_form_mapping?.original_reader_module!==r.reader_module||r.legacy_form_mapping.original_history_or_CSV_modified!==false||!integer(r.source_context_offset)||!hash(r.source_EXC_sha256)||!Array.isArray(r.source_variants)||r.source_variants.length>100)throw Error('Historical source-version context did not match');
      const seen=new Set();
      const versions=r.source_variants.map(v=>{
        const q=v?.variant_name==='Production'?0:v?.variant_name==='SnapshotFederal'?7:null,id=[q,v?.outer_plan,v?.variant_occurrence].join(':');
        if(q===null||v.reader_q_plan!==q||!integer(v.outer_plan)||!integer(v.variant_occurrence)||typeof v.primary_record_present!=='boolean'||seen.has(id)||!Array.isArray(v.source_records)||!v.source_records.length||!Array.isArray(v.E11_forms)||v.E11_forms.length>1000)throw Error('Historical source version did not match');seen.add(id);
        const primary=q===0?30:40;
        for(const source of v.source_records)if(![primary,primary+1].includes(source.record_type)||source.outer_plan!==v.outer_plan||source.personal_prefix_bytes!==182||source.CRC_verified!==true||!integer(source.frame_offset)||![source.frame_sha256,source.expanded_sha256].every(hash))throw Error('Historical version CRC provenance did not match');
        if(v.primary_record_present!==v.source_records.some(s=>s.record_type===primary))throw Error('Historical primary record presence did not match');
        const forms=v.E11_forms.map((f,ordinal)=>{
          if(!f||f.form_name!=='E11'||f.E11_form_ordinal!==ordinal||f.reader_marker_field1_selector_eligible!==true||!integer(f.original_field_count)||!Array.isArray(f.marker_field1_occurrences)||!f.marker_field1_occurrences.length||f.marker_field1_occurrences.some(m=>m.field_id!==1)||!hash(f.source_form_header_sha256)||!f.lines||Object.keys(f.lines).length!==6)throw Error('Historical version E11 form did not match');
          const body=lines.map((line,i)=>{
            const values=f.lines[line];if(values===null)return '<tr><th scope="row">'+labels[i]+' · '+line+'</th><td class="dtmax-missing">Absent</td></tr>';
            if(!Array.isArray(values)||!values.length||values.length>1000)throw Error('Historical field occurrence list did not match');let previous=-1;
            const cells=values.map((field,occurrence)=>{
              if(!field||field.field_id!==Number(line)/100||field.field_id_occurrence!==occurrence||field.type_tag!==2||!Number.isInteger(field.wire_cents)||field.wire_cents< -2147483648||field.wire_cents>2147483647||field.wire_integer!==field.wire_cents||!integer(field.source_field_index)||field.source_field_index<=previous||field.source_field_index>=f.original_field_count||!hash(field.source_field_sha256)||field.source_text!==null||!['explicit_zero','present_wire_cents'].includes(field.state)||(field.state==='explicit_zero')!==(field.wire_cents===0)||!v.source_records.some(s=>s.frame_offset===field.source_EXR_frame_offset&&s.frame_sha256===field.source_EXR_frame_sha256&&s.expanded_sha256===field.source_expanded_record_sha256))throw Error('Historical financial occurrence or frame anchor did not match');
              previous=field.source_field_index;const n=BigInt(field.wire_cents),a=n<0n?-n:n,decimal=(n<0n?'-':'')+(a/100n)+'.'+(a%100n).toString().padStart(2,'0');if(field.source_decimal!==decimal)throw Error('Historical decimal does not match exact wire cents');
              return '<div>'+esc(cents(String(field.wire_cents)))+(values.length>1?'<small>Source occurrence '+(occurrence+1)+'</small>':'')+'</div>';
            }).join('');return '<tr><th scope="row">'+labels[i]+' · '+line+'</th><td>'+cells+'</td></tr>';
          }).join('');
          return '<h4>E11 totals form '+(ordinal+1)+'</h4><div class="table-wrap"><table><thead><tr><th>Line</th><th>Stored amount</th></tr></thead><tbody>'+body+'</tbody></table></div>';
        }).join('');
        return '<details class="dtmax-native-slip"><summary>'+(q===0?'Production':'Federal snapshot')+' · '+r.source_return_year+(v.outer_plan?' · Source plan '+v.outer_plan:'')+(v.variant_occurrence?' · Version occurrence '+(v.variant_occurrence+1):'')+'</summary>'+(forms||'<p>This stored version has no E11 totals form. No amounts have been inferred.</p>')+'<details><summary>Source provenance</summary><p>Native reader version '+esc(v.variant_name)+' · '+v.source_records.length+' CRC-verified record(s). Original archive '+esc(r.source_archive_sha256)+'.</p></details></details>';
      }).join('');
      return '<section><h3>Stored source versions · '+r.source_return_year+'</h3>'+(versions||'<p>No calculated source version was retained for this context. Missing amounts remain unavailable.</p>')+'</section>';
    }).join('');
  }
  function validatePrintedFigures(r){
    const hash=s=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s);
    const byteLength=s=>encodeURIComponent(s).replace(/%[0-9A-F]{2}/g,'x').length;
    const keys=(v,names)=>v&&typeof v==='object'&&!Array.isArray(v)&&Object.keys(v).sort().join('|')===names.slice().sort().join('|');
    if(!keys(r,['schema','printed_figure_profile_sha256','client_number','family_file_id','source_return_year','pdf','anchors','geometry_evidence_json','observations','source_fields_only','recalculated','as_filed_verified','preferred_version_selected','financial_values_approved'])||!keys(r.pdf,['client_number','tax_year','sha256','bytes','page_count','page_labels','export_provenance'])||!keys(r.anchors,['source_backup_sha256','source_archive_sha256','source_figures_csv_sha256','native_family_file_sha256','native_family_file_relative_path','pristine_manifest_sha256','source_context_row_sha256','source_qa_file_sha256','source_qa_record_sha256','source_geometry_sha256','geometry_verifier_sha256','official_cra_form_sha256','supplemental_identity_proof_sha256']))throw Error('Printed observation contract keys did not match');
    const p=r?.pdf,a=r?.anchors;
    if(!r||r.schema!=='dtmax-printed-source-figure-observation/v1'||r.printed_figure_profile_sha256!==printedFigureProfile||!/^\d{1,12}$/.test(r.client_number)||!/^\d{1,12}$/.test(r.family_file_id)||![2023,2024].includes(r.source_return_year)||r.source_fields_only!==true||['recalculated','as_filed_verified','preferred_version_selected','financial_values_approved'].some(k=>r[k]!==false)||!p||p.client_number!==r.client_number||p.tax_year!==r.source_return_year||!hash(p.sha256)||!integer(p.bytes)||!p.bytes||p.bytes>2097152||!integer(p.page_count)||p.page_count<1||p.page_count>500||!Array.isArray(p.page_labels)||p.page_labels.length!==p.page_count||p.page_labels.some(s=>typeof s!=='string'||!s||byteLength(s)>120||/[\u0000-\u001f\u007f]/.test(s)))throw Error('Printed PDF observation context did not match');
    if(!a||['source_backup_sha256','source_archive_sha256','source_figures_csv_sha256','native_family_file_sha256','pristine_manifest_sha256','source_context_row_sha256','source_qa_file_sha256','source_qa_record_sha256','source_geometry_sha256','geometry_verifier_sha256'].some(k=>!hash(a[k]))||['official_cra_form_sha256','supplemental_identity_proof_sha256'].some(k=>a[k]!==null&&!hash(a[k]))||typeof a.native_family_file_relative_path!=='string'||!a.native_family_file_relative_path||a.native_family_file_relative_path.length>200||/:/.test(a.native_family_file_relative_path)||a.native_family_file_relative_path.split(/[\\/]/).some(x=>!x||x==='.'||x==='..')||a.source_backup_sha256!=='7b760c8406ac29f6942898818c28be03c823128fdb164b5b03885f9f37b0eed5'||a.source_archive_sha256!=='499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c'||a.source_figures_csv_sha256!=='c16c0bdc3036932f54e09cdd698513344c945cc4e8ba006cca54cd2fe8f93a94')throw Error('Printed figure source proof did not match');
    const v=p.export_provenance;
    validatePdf({...p,original_print_output:true,immutable:true},r.client_number);
    if(!v||v.kind!=='cached_native_export'||v.source_backup_sha256!==a.source_backup_sha256||v.native_family_file_sha256!==a.native_family_file_sha256||v.figures_csv_sha256!==a.source_figures_csv_sha256||v.recalculated!==false||v.as_filed_verified!==false||!hash(v.export_log_sha256)||!v.figure_comparisons||Object.keys(v.figure_comparisons).length!==6||typeof r.geometry_evidence_json!=='string'||r.geometry_evidence_json.length>131072||!JSON.parse(r.geometry_evidence_json)||!Array.isArray(r.observations)||r.observations.length!==6)throw Error('Printed PDF export provenance did not match');
    const geometry=JSON.parse(r.geometry_evidence_json);if(!geometry||typeof geometry!=='object'||Array.isArray(geometry))throw Error('Printed geometry evidence did not match');
    for(const [i,o] of r.observations.entries()){
      if(!keys(o,['line_code','state','printed_decimal','printed_cents','printed_candidates_json','printed_candidates_sha256','source_comparison_state','source_presence_state']))throw Error('Printed line contract keys did not match');
      if(!o||o.line_code!==lines[i]||o.source_comparison_state!==v.figure_comparisons[lines[i]]||!['present','absent'].includes(o.source_presence_state)||!hash(o.printed_candidates_sha256)||typeof o.printed_candidates_json!=='string'||o.printed_candidates_json.length>65536)throw Error('Printed line observation did not match');
      const candidates=JSON.parse(o.printed_candidates_json);
      if(!Array.isArray(candidates)||candidates.length>100||candidates.some(c=>{const b=c?.line_code_bbox??c?.code_bbox;return !c||!integer(c.page_index)||c.page_index>=p.page_count||c.printed_code!==o.line_code||!Array.isArray(b)||b.length!==4||b.some(x=>typeof x!=='number'||!Number.isFinite(x)||x<0||x>20000)||b[0]>=b[2]||b[1]>=b[3]||!['printed_amount_present','identified_printed_amount_blank','held_ambiguous_money_tokens','held_invalid_money_tokens','held_money_token_crosses_box_boundary'].includes(c.state);}))throw Error('Printed page or geometry evidence did not match');
      if(o.state==='single_distinct_printed_amount'){
        cents(o.printed_cents);const n=BigInt(o.printed_cents),abs=n<0n?-n:n,decimal=(n<0n?'-':'')+(abs/100n)+'.'+(abs%100n).toString().padStart(2,'0');
        if(o.printed_decimal!==decimal||!candidates.length||candidates.some(c=>c.state!=='printed_amount_present'))throw Error('Printed amount or decimal did not match');
      }else{
        if(o.printed_cents!==null||o.printed_decimal!==null)throw Error('Held or absent printed amount must remain null');
        if(o.state==='identified_printed_blank'){if(!candidates.length||candidates.some(c=>c.state!=='identified_printed_amount_blank'))throw Error('Printed blank evidence did not match');}
        else if(o.state==='printed_line_not_located'){if(candidates.length)throw Error('Unlocated line carries candidates');}
        else if(!['held_ambiguous_printed_amount','held_invalid_printed_money_token'].includes(o.state)||!candidates.length)throw Error('Unsupported printed observation state');
      }
    }return r;
  }
  function printedCachedFiguresHtml(records){
    const comparisons={verified_equal_source:'Matches extract figure',printed_source_difference:'Differs from extract figure',comparison_unavailable_source_absent:'Extract figure absent',comparison_unavailable_printed_blank:'Printed line blank',comparison_unavailable_printed_line_not_located:'Printed line not located',held_ambiguous_or_unverified_source_figures:'Extract figure needs review',held_source_numeric_representation:'Extract number needs review',held_ambiguous_printed_amount:'Printed amount needs review'};
    return '<p>Amounts observed on DT Max’s cached printed pages. These remain separate from historical totals and stored calculation versions. The PDF was exported without recalculation; filed-copy status and financial approval are unverified.</p>'+records.map(input=>{
      const r=validatePrintedFigures(input),p=r.pdf;
      const body=r.observations.map((o,i)=>{
        const candidates=JSON.parse(o.printed_candidates_json),pages=[...new Set(candidates.map(c=>c.page_index+1))];
        const label=o.state==='single_distinct_printed_amount'?cents(o.printed_cents):o.state==='identified_printed_blank'?'Blank on printed page':o.state==='printed_line_not_located'?'Printed line not located':'Printed amount needs review';
        return '<tr><th scope="row">'+labels[i]+' · '+o.line_code+'</th><td>'+esc(label)+'</td><td>'+pages.map(page=>'<button class="dtmax-action" type="button" data-printed-pdf-sha="'+p.sha256+'" data-printed-pdf-year="'+p.tax_year+'" data-printed-pdf-page="'+page+'">View printed page '+page+'</button>').join(' ')+'<small>'+esc(comparisons[o.source_comparison_state])+'</small></td></tr>';
      }).join('');
      return '<details class="dtmax-native-slip"><summary>Printed cached figures · '+r.source_return_year+' · '+p.page_count+' pages</summary><div class="table-wrap"><table><thead><tr><th>Printed line</th><th>Observed amount</th><th>Source page and comparison</th></tr></thead><tbody>'+body+'</tbody></table></div><details><summary>PDF and observation provenance</summary><p>PDF SHA256 '+esc(p.sha256)+'</p><p>Geometry proof '+esc(r.anchors.source_geometry_sha256)+'<br>Source QA '+esc(r.anchors.source_qa_record_sha256)+'</p><p>Missing or held amounts remain unavailable. No amount was inferred from a blank and no source version was selected.</p></details></details>';
    }).join('');
  }
  const missing='<span class="dtmax-missing">Not imported</span>';
  function keywordRows(rows){return '<dl>'+rows.map(([label,value])=>'<dt data-dtmax-keyword="'+esc(label.toLowerCase())+'">'+esc(label)+'</dt><dd>'+(value===undefined||value===null||value===''?missing:esc(value))+'</dd>').join('')+'</dl>';}
  function personalInfoHtml(client,record){
    const p=record?.source_personal_info;
    if(!p)return '<details><summary>7 personal fields not imported</summary>'+keywordRows([['Relation',null],['SIN',null],['BirthDate',null],['Status',null],['Sex',null],['Prov-Residence',null],['Language',null]])+'</details>';
    const names=['Relation','SIN','BirthDate','Status','Sex','Prov-Residence','Language'];
    if(p.schema!=='dtmax-personal-source-display/v1'||p.binding_profile_sha256!=='9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e'||p.client_number!==client.client_number||p.client_number!==record.client_number||p.family_file_id!==record.family_file_id||p.source_year!==record.tax_year||p.archive_sha256!==record.archive_sha256||p.reader_module!==record.vendor_module||p.info_record_state!=='present_nonempty_crc_verified'||p.current_profile_updated!==false||p.identity_verified!==false||!p.fields||Object.keys(p.fields).length!==7||names.some(n=>!p.fields[n])||p.t1_display_eligible!==(p.reader_module==='T1')||p.display_state!==(p.t1_display_eligible?'eligible_t1_source_year':'held_module_origin'))throw Error('Historical personal information did not match its source context');
    const supported=['present_exact_date_candidate','present_mapped_code','present_nine_digit_candidate_not_identity_verified'];
    const reviewedOrigin=nativeModuleReviewed(record);
    const rows=names.map(name=>{
      const f=p.fields[name],eligible=p.t1_display_eligible&&f.candidate!==null&&supported.includes(f.state);
      if(f.display_eligible!==eligible)throw Error('Personal source display state did not match');
      if(name==='SIN'&&(f.raw_code!==null||f.raw_redacted!==true||f.candidate!==null&&!/^\*{3}-\*{3}-[0-9]{4}$/.test(f.candidate)))throw Error('Personal source SIN was not masked');
      let value;
      if(!p.t1_display_eligible&&!reviewedOrigin)value='Source module needs review';
      else if(!(eligible||reviewedOrigin&&f.candidate!==null&&supported.includes(f.state)))value=f.state==='present_blank'?'Blank in source':f.state==='source_placeholder_zero_sentinel'?'Placeholder in source':f.state==='present_invalid_calendar_date'?'Invalid date in source':'Unknown or unsupported source value';
      else if(name==='Relation'){
        if(!f.candidate||Object.keys(f.candidate).sort().join(',')!=='relationship,taxpayer_type'||!['Principal','Partner','Dependant'].includes(f.candidate.taxpayer_type)||typeof f.candidate.relationship!=='string')throw Error('Personal source relation is invalid');
        value=f.candidate.taxpayer_type+(f.candidate.relationship==='None'?'':' · '+f.candidate.relationship);
      }else{
        if(typeof f.candidate!=='string'||f.candidate.length>100||name==='BirthDate'&&!/^\d{4}-\d{2}-\d{2}$/.test(f.candidate))throw Error('Personal source candidate is invalid');
        value=f.candidate+(name==='Prov-Residence'&&f.residency?' · '+f.residency:'');
      }
      return [name==='Status'?'Status (marital)':name==='Prov-Residence'?'Prov-Residence (Dec 31)':name,value];
    });
    return '<h3>Historical personal information · '+p.source_year+'</h3>'+keywordRows(rows)+'<p>Source-year values. SIN is masked; current client profile is unchanged.'+(reviewedOrigin?' Native T1 origin is independently verified; the original converter label and stored flags are retained.':'')+'</p>';
  }
  function entryHtml(client,record,comparison){
    const profile=client.source_profile||{};
    const totals=historicalTotalsHtml;
    return '<div class="dtmax-entry"><section class="dtmax-keyword-pane"><h3>Personal information</h3>'+keywordRows([['Client #',client.client_number],['Name',client.name]])+personalInfoHtml(client,record)+totals(record)+'<h3>Mailing address · client-list snapshot</h3>'+keywordRows([['Street',profile.street],['City',profile.city],['Province',profile.province],['PostalCode',profile.postal_code],['Country',profile.country]])+'<h3>Contact · client-list snapshot</h3>'+keywordRows([['Email',profile.email],['Home phone',profile.home_phone],['Work phone',profile.work_phone],['Cell phone',profile.cell_phone]])+'<p>Address, contact and processing status come from the imported client list. They are not established as the historical return’s values.</p></section><section class="dtmax-keyword-pane"><label>Compare historical year <select data-dtmax-compare><option value="">&lt;none&gt;</option>'+(client.historical_years||[]).map(y=>'<option value="'+y+'"'+(String(y)===comparison?' selected':'')+'>'+y+'</option>').join('')+'</select></label><div data-dtmax-comparison></div><h3>Source records</h3><p>Browse the preserved slips and family links below. Source values are read-only.</p><div data-dtmax-details></div></section></div>';
  }
  function validatePdf(r,client){
    const office=r?.retained_office_provenance;
    if(!r||r.client_number!==client||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||! /^[a-f0-9]{64}$/.test(r.sha256)||r.original_print_output!==(office===undefined)||r.immutable!==true||!integer(r.bytes)||r.bytes===0||r.bytes>2097152||!Array.isArray(r.page_labels)||r.page_labels.length!==r.page_count||r.page_count<1||r.page_count>500||r.page_labels.some(x=>typeof x!=='string'||!x||x.length>120||/[\x00-\x1f]/.test(x)))throw Error('Original PDF identity or page index did not match');
    if(office!==undefined){
      const hashes=['source_manifest_sha256','source_occurrence_manifest_sha256','owner_year_evidence_sha256','mixed_owner_year_review_sha256','static_pdf_review_sha256','source_metadata_sha256'];
      if(!office||!((office.kind==='retained_office_T1_document'&&office.profile_sha256==='b51cc8aec7d930f6e5b4d05575538f87856a39c600f5893a0828aff96816812e')||(office.kind==='retained_office_T1_stable_identifier_document'&&office.profile_sha256==='8b45d6655423ff1f2e0f8202a1009f25b1e472c73f7b7591b581a73264f42028')||(office.kind==='retained_office_T1_scanned_document'&&office.profile_sha256==='606e38ceaa60949c7d631761beb49bd6afc5c95fb56fe1b0a9290a78062c0480'))||r.export_provenance!==undefined||Object.keys(office).sort().join('|')!==['kind','profile_sha256',...hashes,'native_generation_verified','as_filed_verified','financial_values_approved','source_bytes_modified'].sort().join('|')||hashes.some(k=>typeof office[k]!=='string'||!/^[a-f0-9]{64}$/.test(office[k]))||['native_generation_verified','as_filed_verified','financial_values_approved','source_bytes_modified'].some(k=>office[k]!==false))throw Error('Retained office T1 provenance did not match');
    }
    const p=r.export_provenance;
    if(p!==undefined&&(!p||p.kind!=='cached_native_export'||p.source_backup_sha256!=='7b760c8406ac29f6942898818c28be03c823128fdb164b5b03885f9f37b0eed5'||![p.native_family_file_sha256,p.export_log_sha256].every(s=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s))||p.recalculated!==false||p.as_filed_verified!==false||typeof p.exported_at_utc!=='string'||!/^\d{4}-\d{2}-\d{2}T.*Z$/.test(p.exported_at_utc)||!Number.isFinite(Date.parse(p.exported_at_utc))))throw Error('Cached PDF export provenance did not match');
    if(p!==undefined&&(p.figures_csv_sha256!=='c16c0bdc3036932f54e09cdd698513344c945cc4e8ba006cca54cd2fe8f93a94'||!p.figure_comparisons||Object.keys(p.figure_comparisons).length!==6||lines.some(l=>!['verified_equal_source','printed_source_difference','comparison_unavailable_source_absent','comparison_unavailable_printed_blank','comparison_unavailable_printed_line_not_located','held_ambiguous_or_unverified_source_figures','held_source_numeric_representation','held_ambiguous_printed_amount'].includes(p.figure_comparisons[l]))))throw Error('Cached PDF figure comparison did not match');
    return r;
  }
  function pdfSourceLabel(r){return r.retained_office_provenance?(r.retained_office_provenance.kind==='retained_office_T1_scanned_document'?'Retained office T1 scan · Visual source review; native generation, return completeness and filed-copy status unverified':r.retained_office_provenance.kind==='retained_office_T1_stable_identifier_document'?'Retained office T1 · Taxpayer identifier continuity; original name match unverified · Native generation, return completeness and filed-copy status unverified':'Retained office T1 · Native generation, return completeness and filed-copy status unverified'):r.export_provenance?'Cached native export':'Retained print output';}
  function validateException(r){
    const digest=s=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s);
    if(!r||!digest(r.source_archive_sha256)||!digest(r.source_context_sha256)||!digest(r.source_year_frame_sha256)||!integer(r.source_context_offset)||!integer(r.source_year_frame_offset)||r.source_year_frame_offset<56||r.source_year_frame_offset+12>r.source_context_offset||!integer(r.source_archive_bytes)||!integer(r.source_context_bytes)||r.source_context_bytes<70||r.source_context_bytes>65536||r.source_context_offset+r.source_context_bytes>r.source_archive_bytes||!/^\d{1,10}$/.test(r.client_number)||!/^\d{1,10}$/.test(r.family_file_id)||!Number.isInteger(r.tax_year)||r.tax_year<1990||r.tax_year>2100||!['missing_identity_information','empty_information_payload'].includes(r.reason)||r.immutable!==true||r.identity_metadata_readable!==false||r.normalized_amounts_created!==false||r.header_ids_are_source_references_only!==true||!Array.isArray(r.frame_tags)||r.frame_tags.length<2||r.frame_tags.length>256||r.frame_tags[0]!=='EXC'||r.frame_tags[1]!=='EXP'||r.frame_tags.slice(2).some(t=>!['EXR','EXA'].includes(t)))throw Error('Unreadable source context metadata did not match');return r;
  }
  function cachedFigureNotice(p){
    const states=p.figure_comparisons,different=lines.filter(l=>states[l]==='printed_source_difference'),unavailable=lines.filter(l=>!['verified_equal_source','comparison_unavailable_source_absent','printed_source_difference'].includes(states[l]));
    const message=different.length?'This cached PDF differs from the imported source totals on line(s) '+different.join(', ')+'. Compare the source versions before using these figures.'+(unavailable.length?' Other printed figures could not be compared.':''):unavailable.length?'Some printed figures could not be compared with the imported totals. Review the source versions before using these figures.':'Every available source figure matched its printed amount. Missing source values remain unavailable.';
    return '<p class="dtmax-notice"'+(different.length?' role="alert"':'')+'>'+esc(message)+'</p>';
  }
  function exceptionsHtml(records){
    records.forEach(validateException);
    if(!records.length)return '<p>No unreadable source contexts are preserved in this scope.</p>';
    return '<p>Original archive fragments are preserved below. Header IDs are source references; they do not establish a person, ownership or tax figures.</p><div class="table-wrap"><table><thead><tr><th>Year</th><th>Header client #</th><th>Family file #</th><th>Source reader exception</th><th>Original bytes</th></tr></thead><tbody>'+records.map((r,i)=>'<tr><td>'+r.tax_year+'</td><td>'+esc(r.client_number)+'</td><td>'+esc(r.family_file_id)+'</td><td>'+esc(r.reason==='missing_identity_information'?'Identity information missing':'Identity information payload empty')+(r.frame_tags.includes('EXA')?'<small>Opaque family auxiliary data included</small>':'')+'</td><td><button type="button" data-exception-download="'+i+'">Download fragment · '+r.source_context_bytes+' bytes</button></td></tr>').join('')+'</tbody></table></div>';
  }
  function mount(host,{request,isCurrent=()=>true}) {
    let sequence=0,offset=0,search='',year='',activeClient='',kind='historical-lines',factOffset=0;
    let page=null,view='list',selectedYear='',comparison='',history=[],pdfs=[],pdfUrl='',pdfRecord=null,pdfPage=1;
    let sort='client_number',direction='asc',status='',province='',exceptionSequence=0,ownerSequence=0;const clientCache=new Map(),exceptionUrls=[];
    const releaseExceptions=()=>{exceptionUrls.splice(0).forEach(url=>URL.revokeObjectURL(url));};
    const current=n=>host.isConnected!==false&&isCurrent()&&n===sequence;
    const q=s=>host.querySelector(s),all=s=>host.querySelectorAll(s);
    const selected=()=>clientCache.get(activeClient);
    const releasePdf=()=>{if(pdfUrl){URL.revokeObjectURL(pdfUrl);pdfUrl='';}pdfRecord=null;};
    function tabs(){return '<div class="dtmax-tabs" role="tablist" aria-label="Imported T1 workspace">'+[['list','Client list','F10'],['entry','Data entry','F2'],['return','Tax return','F9']].map(([id,label,key])=>'<button role="tab" id="dtmax-tab-'+id+'" aria-controls="dtmax-panel" aria-selected="'+(id===view)+'" tabindex="'+(id===view?'0':'-1')+'" data-dtmax-view="'+id+'"'+(id!=='list'&&!activeClient?' disabled':'')+'>'+label+'<kbd>'+key+'</kbd></button>').join('')+'<span class="dtmax-module">T1</span></div>';}
    function shell(body){
      if(view==='list')body='<details data-dtmax-owner-review><summary>Legacy CSV ownership reconciliation</summary><div data-dtmax-owner-content><p>Open to load verified ownership counts.</p></div></details><details data-dtmax-exceptions><summary>Unreadable source contexts</summary><div data-dtmax-exception-content><p>Open to load preserved source exceptions.</p></div></details>'+body;
      host.innerHTML='<section class="dtmax-workspace" aria-label="DT Max source workspace"><div class="dtmax-caption"><h2>T1 clients imported from DT Max</h2><span>Firm-wide source workspace</span></div>'+tabs()+'<p class="dtmax-notice">Preserved source records · Read-only. NXT preparation workpapers remain separate below.</p><div id="dtmax-panel" role="tabpanel" aria-labelledby="dtmax-tab-'+view+'">'+body+'</div></section>';
      all('[data-dtmax-view]').forEach(b=>{b.onclick=()=>show(b.dataset.dtmaxView);b.onkeydown=e=>{if(e.key==='ArrowRight'||e.key==='ArrowLeft'){e.preventDefault();const ids=activeClient?['list','entry','return']:['list'];const at=ids.indexOf(view);return show(ids[(at+(e.key==='ArrowRight'?1:ids.length-1))%ids.length],true);}};});
      if(view==='list')q('[data-dtmax-exceptions]').ontoggle=e=>{if(e.target.open)return loadExceptions(0);};
      if(view==='list')q('[data-dtmax-owner-review]').ontoggle=e=>{if(e.target.open)return loadOwnerReview();};
      host.onkeydown=e=>{
        if(e.ctrlKey&&!e.altKey&&!e.metaKey&&e.key.toLowerCase()==='f'&&view==='list'){e.preventDefault();q('[name=search]')?.focus?.();return;}
        if(e.altKey&&e.key==='F7'&&view==='entry'){e.preventDefault();q('[data-dtmax-keyword-find]')?.focus?.();return;}
        if(e.altKey||e.ctrlKey||e.metaKey||/INPUT|SELECT|TEXTAREA/.test(e.target?.tagName||''))return;const id={F10:'list',F2:'entry',F9:'return'}[e.key];if(id&&(activeClient||id==='list')){e.preventDefault();show(id,true);}
      };
    }
    async function loadOwnerReview(){
      const n=sequence,g=++ownerSequence,detail=q('[data-dtmax-owner-content]');if(!detail)return;detail.innerHTML='<p>Loading owner reconciliation…</p>';
      try{
        const p=await request(BASE+'legacy-owner-links/summary');if(!current(n)||view!=='list'||g!==ownerSequence)return;
        if(p.source_scope!==SCOPE||p.test_copy!==true||p.source_rows_modified!==0||p.normalized_slips_created!==0||p.tax_cases_imported!==0||![p.raw_legacy_CSV_owner_ids_unregistered,p.verified_owner_links,p.remaining_owner_exceptions].every(integer)||p.verified_owner_links+p.remaining_owner_exceptions!==p.raw_legacy_CSV_owner_ids_unregistered)throw Error('Owner reconciliation did not match');
        detail.innerHTML='<p>'+p.verified_owner_links+' of '+p.raw_legacy_CSV_owner_ids_unregistered+' CSV source rows have verified owner links. '+p.remaining_owner_exceptions+' ownership exceptions remain.</p><p>Open a linked client’s Data entry → Linked T5008 CSV. Original CSV client references remain preserved; these links do not create additional slips or approve tax figures.</p>';
      }catch(e){if(current(n)&&view==='list'&&g===ownerSequence)detail.innerHTML='<p role="alert">Owner reconciliation unavailable: '+esc(e.message)+'</p>';}
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
    async function show(next,focus=false,requestedPdf=null){
      if(next!=='list'&&!selected())return;const n=++sequence;view=next;
      if(next==='list'){releasePdf();renderList();}
      else if(next==='entry'){
        releasePdf();shell(clientHead()+'<div class="dtmax-toolbar"><label>Find keyword <input type="search" data-dtmax-keyword-find placeholder="Keyword or line number · Alt+F7"></label><span class="dtmax-missing" data-dtmax-keyword-count></span></div>'+entryHtml(selected(),historicalRecord(selectedYear),comparison));bindClient();
        q('[data-dtmax-keyword-find]').oninput=e=>{const needle=e.target.value.trim().toLowerCase();let count=0;all('[data-dtmax-keyword]').forEach(dt=>{const match=!needle||dt.dataset.dtmaxKeyword.includes(needle);dt.hidden=!match;if(dt.nextElementSibling)dt.nextElementSibling.hidden=!match;if(match)count++;});q('[data-dtmax-keyword-count]').textContent=needle?count+' matching fields':'';};
        if(history.filter(r=>String(r.tax_year)===selectedYear).length>1)q('[data-dtmax-comparison]').innerHTML='<p role="alert">Multiple source versions exist for this year. Review Historical totals below; no version was selected automatically.</p>';
        q('[data-dtmax-compare]').onchange=e=>{comparison=e.target.value;renderComparison();};renderComparison();await facts(0,n);
      }else{
        shell(clientHead()+'<div class="dtmax-return"><aside class="dtmax-page-index"><h3>Select a page</h3><div data-dtmax-pdf-index>Loading original PDFs…</div></aside><div class="dtmax-pdf-canvas" data-dtmax-pdf-canvas><div class="dtmax-pdf-message">Loading preserved print output…</div></div></div>');bindClient();
        try{const p=await request(BASE+'clients/'+activeClient+'/return-pdfs');if(!current(n))return;if(p.source_scope!==SCOPE||p.client_number!==activeClient||p.test_copy!==true||p.tax_cases_imported!==0||!Array.isArray(p.records)||p.records.length>1000)throw Error('Original PDF list did not match');pdfs=p.records.map(r=>validatePdf(r,activeClient));releasePdf();renderPdfIndex();const versions=pdfs.filter(r=>String(r.tax_year)===selectedYear);const r=requestedPdf?versions.find(r=>r.sha256===requestedPdf.sha):versions.length===1?versions[0]:null;if(requestedPdf&&(!r||!integer(requestedPdf.page)||requestedPdf.page<1||requestedPdf.page>r.page_count))throw Error('Observed PDF page is unavailable');if(r){await openPdf(r,n);if(requestedPdf&&current(n)){pdfPage=requestedPdf.page;renderPdfIndex();renderPdfCanvas();}}else{releasePdf();if(versions.length>1)pdfChoose(versions);else pdfMissing();}}catch(e){if(current(n)){releasePdf();q('[data-dtmax-pdf-index]').innerHTML='Unavailable';q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message" role="alert">Original PDF unavailable: '+esc(e.message)+'</div>';}}
      }
      if(focus&&current(sequence))q('[data-dtmax-view="'+view+'"]')?.focus?.();
    }
    function renderComparison(){const node=q('[data-dtmax-comparison]');if(!node)return;const r=historicalRecord(comparison);node.innerHTML=comparison?(r?historicalTotalsHtml(r):'<p>No single imported source version for this comparison year.</p>'):'<p class="dtmax-missing">Choose a year to compare the six preserved totals.</p>';}
    function checkFacts(p,client,requestedKind,at,limit,requestedYear){
      if(!p||p.client_number!==client||p.kind!==requestedKind||p.test_copy!==true||p.tax_cases_imported!==0||p.offset!==at||!integer(p.total)||!Array.isArray(p.records)||p.records.length>limit||at+p.records.length>p.total)throw Error('Source client response did not match');
      if(p.records.some(r=>requestedKind==='family-files'?!Array.isArray(r.member_client_numbers)||!r.member_client_numbers.includes(client):requestedKind==='historical-family-lineage'?!Array.isArray(r.member_mappings)||!r.member_mappings.some(m=>m.current_client_number===client):r.client_number!==client))throw Error('Source record belongs to another client');
      if(['native-slip-views','linked-t5008','historical-variants','printed-cached-figures'].includes(requestedKind)&&requestedYear&&(p.source_return_year!==Number(requestedYear)||p.records.some(r=>(requestedKind==='linked-t5008'?r.tax_year:r.source_return_year)!==Number(requestedYear))))throw Error('Selected source return year did not match');
      if(requestedKind==='printed-cached-figures'&&(p.source_scope!==SCOPE||p.printed_figure_profile_sha256!==printedFigureProfile||p.original_history_modified!==0||p.original_PDFs_modified!==0||p.current_profiles_modified!==0||p.financial_values_approved!==false||p.preferred_version_selected!==false||p.as_filed_verified!==false))throw Error('Printed figure response did not match');
      if(requestedKind==='historical-variants'&&(p.source_variant_profile_sha256!=='641c2b5f56617882829d5049813e06789a3a661580fe6f6d3adf6f8dde5e03ba'||p.original_history_modified!==0||p.personal_source_flags_modified!==0||p.financial_values_approved!==false||p.preferred_version_selected!==false))throw Error('Historical source-version response did not match');
    }
    async function facts(next=0,existingSequence){
      const n=existingSequence??++sequence,client=activeClient,requestedKind=kind,requestedYear=['native-slip-views','linked-t5008','historical-variants','printed-cached-figures'].includes(kind)?selectedYear:'';factOffset=next;const detail=q('[data-dtmax-details]');if(!detail)return;detail.innerHTML='<p>Loading imported T1 records…</p>';
      try{
        const p=await request(BASE+'clients/'+client+'/source-facts?kind='+requestedKind+'&offset='+next+'&limit=25'+(requestedYear?'&source_return_year='+encodeURIComponent(requestedYear):''));if(!current(n))return;checkFacts(p,client,requestedKind,next,25,requestedYear);
        detail.innerHTML='<div class="dtmax-fact-nav">'+Object.entries(kinds).map(([k,label])=>'<button type="button" data-source-kind="'+k+'" aria-pressed="'+(k===kind)+'">'+label+'</button>').join('')+'</div>'+recordsHtml(kind,p.records)+'<p>'+(p.total?next+1:0)+'–'+(next+p.records.length)+' of '+p.total+' source records</p><button class="dtmax-action" data-fact-prev '+(!next?'disabled':'')+'>Previous records</button> <button class="dtmax-action" data-fact-next '+(next+p.records.length>=p.total?'disabled':'')+'>Next records</button>';
        detail.querySelectorAll('[data-source-kind]').forEach(b=>b.onclick=()=>{kind=b.dataset.sourceKind;return facts(0);});detail.querySelector('[data-fact-prev]').onclick=()=>facts(Math.max(0,factOffset-25));detail.querySelector('[data-fact-next]').onclick=()=>facts(factOffset+25);
        detail.querySelectorAll('[data-family-client]').forEach(b=>b.onclick=()=>open(b.dataset.familyClient));
        detail.querySelectorAll('[data-printed-pdf-sha]').forEach(b=>b.onclick=()=>{selectedYear=b.dataset.printedPdfYear;return show('return',false,{sha:b.dataset.printedPdfSha,page:Number(b.dataset.printedPdfPage)});});
      }catch(e){if(current(n))detail.innerHTML='<p role="alert">Imported T1 records unavailable: '+esc(e.message)+'</p>';}
    }
    function pdfMissing(){q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message"><h3>Original printed return not imported for '+esc(selectedYear||'this year')+'</h3><p>The historical totals and slips are available in Data entry. A complete printed return needs the original DT Max PDF for this client and year.</p><p>No return pages have been reconstructed from incomplete figures.</p></div>';}
    function pdfChoose(versions){q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message"><h3>Choose a preserved document</h3><p>'+versions.length+' documents are available for '+esc(selectedYear)+'. Choose a copy from the document selector. No preferred version has been selected.</p><ul>'+versions.map(r=>'<li>'+esc(pdfSourceLabel(r))+' · '+r.page_count+' pages · SHA256 '+esc(r.sha256.slice(0,12))+'</li>').join('')+'</ul></div>';}
    function renderPdfIndex(){
      const node=q('[data-dtmax-pdf-index]');node.innerHTML='<label>Preserved T1 PDFs<select data-dtmax-pdf-version><option value="">Choose a preserved document</option>'+pdfs.map((r,i)=>'<option value="'+i+'"'+(pdfRecord?.sha256===r.sha256&&pdfRecord?.tax_year===r.tax_year?' selected':'')+'>'+r.tax_year+' · '+r.page_count+' pages · '+esc(pdfSourceLabel(r))+' · Version '+(i+1)+'</option>').join('')+'</select></label>'+(pdfRecord?pdfRecord.page_labels.map((label,i)=>'<button type="button" data-dtmax-page="'+(i+1)+'" aria-current="'+(pdfPage===i+1?'page':'false')+'">'+(i+1)+'. '+esc(label)+'</button>').join(''):'<p class="dtmax-missing">'+(pdfs.length?'Choose an available PDF above.':'No printed returns imported for this client.')+'</p>');
      q('[data-dtmax-pdf-version]').onchange=e=>{const r=pdfs[Number(e.target.value)];if(e.target.value!==''&&r){selectedYear=String(r.tax_year);return show('return',false,{sha:r.sha256,page:1});}};
      all('[data-dtmax-page]').forEach(b=>b.onclick=()=>{pdfPage=Number(b.dataset.dtmaxPage);renderPdfCanvas();renderPdfIndex();});
    }
    async function openPdf(record,n){
      const client=activeClient;releasePdf();q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message">Verifying original PDF bytes…</div>';
      try{const p=await request(BASE+'clients/'+client+'/return-pdfs/'+record.tax_year+'/'+record.sha256);if(!current(n))return;const r=validatePdf(p.record,client);
        if(p.source_scope!==SCOPE||p.test_copy!==true||p.tax_cases_imported!==0||r.sha256!==record.sha256||r.tax_year!==record.tax_year||r.original_print_output!==record.original_print_output||JSON.stringify(r.page_labels)!==JSON.stringify(record.page_labels)||JSON.stringify(r.export_provenance)!==JSON.stringify(record.export_provenance)||JSON.stringify(r.retained_office_provenance)!==JSON.stringify(record.retained_office_provenance)||typeof p.content_hex!=='string'||p.content_hex.length!==r.bytes*2||!/^[0-9a-f]+$/.test(p.content_hex))throw Error('Original PDF content did not match');
        const bytes=Uint8Array.from(p.content_hex.match(/../g),s=>parseInt(s,16));const actual=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');if(!current(n))return;if(actual!==r.sha256)throw Error('Original PDF failed browser hash verification');
        pdfUrl=URL.createObjectURL(new Blob([bytes],{type:'application/pdf'}));pdfRecord=r;pdfPage=1;renderPdfIndex();renderPdfCanvas();
      }catch(e){if(current(n)){releasePdf();q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-message" role="alert">Original PDF unavailable: '+esc(e.message)+'</div>';}}
    }
    function renderPdfCanvas(){
      if(!pdfUrl||!pdfRecord)return;const title=pdfRecord.page_labels[pdfPage-1],p=pdfRecord.export_provenance,office=pdfRecord.retained_office_provenance,scan=office?.kind==='retained_office_T1_scanned_document';
      q('[data-dtmax-pdf-canvas]').innerHTML='<div class="dtmax-pdf-tools"><p>'+esc(pdfRecord.tax_year)+' · Page '+pdfPage+' of '+pdfRecord.page_count+' · Original bytes verified</p>'+(office?'<h3>Retained office T1 document</h3><p>Preserved as supplied; native generation, completeness of the original return and filed-copy status are unverified. The preserved document bytes are unchanged.</p>'+(pdfRecord.retained_office_provenance.kind==='retained_office_T1_stable_identifier_document'?'<p>Original name match remains unverified; taxpayer identifier continuity matched. This establishes continuity with the archived source, not government identity verification or a legal name change.</p>':pdfRecord.retained_office_provenance.kind==='retained_office_T1_scanned_document'?'<p>Retained office T1 scan. Visual review linked these pages to the archived tax source. This establishes source continuity; government identity, filed-copy status and financial values are unverified.</p>':''):p?'<p>Exported from DT Max’s stored print queue on '+esc(p.exported_at_utc)+'. No recalculation. Filed-copy status is unverified.</p>'+cachedFigureNotice(p):'')+'<a href="'+esc(pdfUrl)+'" download="'+(scan?'retained-office-t1-scan-':office?'retained-office-t1-':'dtmax-t1-')+esc(activeClient)+'-'+pdfRecord.tax_year+'.pdf">'+(scan?'Download retained office T1 scan':office?'Download retained office T1':'Download DT Max PDF')+'</a></div><iframe title="'+(scan?'Retained office T1 scan: ':office?'Retained office T1 document: ':'Original DT Max printed return: ')+esc(title)+'" referrerpolicy="no-referrer" src="'+esc(pdfUrl)+'#page='+pdfPage+'&view=FitH&navpanes=0"></iframe>';
    }
    return {ready:load(),load,facts,show,open,dispose:()=>{++sequence;releasePdf();releaseExceptions();host.onkeydown=null;}};
  }
  globalThis.NXTDTMaxT1={mount,cents,validatePage,recordsHtml,entryHtml,validatePdf,validateException,exceptionsHtml,cachedFigureNotice,validatePrintedFigures};
})();
