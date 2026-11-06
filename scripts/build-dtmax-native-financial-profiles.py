"""Build the checked financial-field allowlist from pinned static evidence only."""
import collections
import hashlib
import json
from pathlib import Path

import argparse
repo=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check',action='store_true',help='Verify the checked-in profile without writing')
args=parser.parse_args()
paths={
    'T4-T5':('dtmax-native-t4-t5-field-profile-evidence.json','457721f49510aff9dad0a6820742c4226142a52ad622ece88aa85586b6001452'),
    'other':('dtmax-other-native-field-profile-evidence.json','54003615d5dc3c62bbe60d066e8d9fe83ec4385290a26c984b49fa8506ebd039'),
}
def canonical(value):return json.dumps(value,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode('utf-8')
evidence={}
for key,(name,pin) in paths.items():
    value=json.loads((repo/'docs/evidence'/name).read_bytes())
    if hashlib.sha256(canonical(value)).hexdigest()!=pin:raise ValueError('Reviewed static evidence differs')
    evidence[key]=value
profiles={}
for source in evidence['T4-T5']['slips']:
    labels=collections.defaultdict(set)
    for assignment in source['federal_money_assignments']:
        label=assignment.get('semantic_name') or 'Amount · box label not verified'
        if assignment.get('target_box'):
            label='Box '+assignment['target_box']+' · '+label
        for call in assignment['source_calls']:
            assert call['helper']=='M'
            labels[call['field_id']].add(label)
    if source['kind']=='T4':
        labels[15079].add('Employment income / issuer · compound source text')
    profiles[source['kind']]={
        'field_ids':sorted(labels),
        'field_labels':{str(key):' / '.join(sorted(values)) for key,values in sorted(labels.items())},
        'selection_review':(
            'Native form order and cached/data origin are preserved. These are source fields, not deduplicated economic slips. '
            'The financial field profile was checked against the source-2025 carryforward reader. '
            +('Pensionable earnings depend on historical ProvinceDec31; second CPP/QPP fields are not supplied by this profile.' if source['kind']=='T4' else 'Taxpayer allocations, country rules and ownership are not applied.')
        ),
    }
for source in evidence['other']['profiles']:
    labels=collections.defaultdict(set)
    allowed=set(source['field_ids'])
    for assignment in source['financial_assignments']:
        label=assignment.get('semantic_name') or 'Amount · box label not verified'
        if assignment.get('target_box'):
            label='Box '+assignment['target_box']+' · '+label
        for call in assignment['source_calls']:
            if call['source_field_id'] in allowed:
                labels[call['source_field_id']].add(label)
    assert set(labels)==allowed,(source['kind'],'missing field label')
    profiles[source['kind']]={
        'field_ids':sorted(allowed),
        'field_labels':{str(key):' / '.join(sorted(values)) for key,values in sorted(labels.items())},
        'selection_review':' '.join(source['selection_review']),
    }
assert len(profiles)==19
assert 13103 not in profiles['T2202A']['field_ids']
out=repo/'src/source_import/native_slip_profiles.json'
if args.check:
    if canonical(json.loads(out.read_bytes()))!=canonical(profiles):raise ValueError('Checked-in financial profiles differ')
else:
    out.write_bytes((json.dumps(profiles,indent=2,ensure_ascii=False)+'\n').encode('utf-8'))
print(json.dumps({'kinds':len(profiles),'financial_field_bindings':sum(len(p['field_ids']) for p in profiles.values()),'canonical_profile_sha256':hashlib.sha256(json.dumps(profiles,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest(),'source_client_data_read':False}))
