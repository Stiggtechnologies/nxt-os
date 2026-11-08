import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash} from 'node:crypto';
const sandbox={};vm.runInNewContext(fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8'),sandbox);const api=sandbox.NXTDTMaxT1;
test('summary observations reject unqualified and unknown records',()=>{for(const r of [null,{},[],{schema:'dtmax-source-summary-figure-observations/v1'}])assert.throws(()=>api.validateSourceSummaryFigures(r));});
test('actual summary shows five exact observed amounts and an unavailable refund',{skip:!process.env.NXT_SUMMARY_FIGURE_OBSERVATION},()=>{
 const raw=fs.readFileSync(process.env.NXT_SUMMARY_FIGURE_OBSERVATION);assert.equal(createHash('sha256').update(raw).digest('hex'),'327ad9bb4037a532cb3da1eaa6276eeb51b98042acf51c924681b19ab67b856b');const r=JSON.parse(raw);api.validateSourceSummaryFigures(r);const html=api.recordsHtml('source-summary-figures',[r]);
 for(const o of r.observations.filter(o=>o.state==='printed_amount_present')){const [whole,minor]=o.printed_decimal.split('.'),value=whole.replace(/\B(?=(\d{3})+(?!\d))/g,',')+'.'+minor;assert.ok(html.includes(value));}assert.match(html,/Not printed — unavailable/);assert.match(html,/complete federal T1 and same-year native return have not been recovered/);assert.match(html,/No amount inferred/);assert.match(html,/data-printed-pdf-page="1"/);
 for(const variant of ['owner','year','summary-role','native','approved','refund-zero','code','geometry','decimal']){const v=structuredClone(r);switch(variant){case'owner':v.client_number='999';break;case'year':v.source_return_year=2006;break;case'summary-role':v.pdf.source_archive_scope.source_role='T1_identification_archive';break;case'native':v.native_context_state='present';break;case'approved':v.financial_values_approved=true;break;case'refund-zero':v.observations[4].printed_cents='0';break;case'code':v.observations[0].printed_line_code='15000';break;case'geometry':v.observations[0].amount_bbox=[10,20,5,20];break;default:v.observations[0].printed_decimal='0.00';}assert.throws(()=>api.validateSourceSummaryFigures(v),variant);}
});
