"""Read original DT Max form packages; no vendor execution or client data access."""
import argparse, hashlib, json, pathlib, struct, xml.etree.ElementTree as ET
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--installation',type=pathlib.Path,default=pathlib.Path(r'C:\Program Files (x86)\DT Max'))
parser.add_argument('--report',type=pathlib.Path)
args=parser.parse_args()
BASE=args.installation
PINNED={
 'DT1WFO13':'9d7254fabd95f679025e3fc7e441304709f259fd7fbfc2425787a231fa3ed07e',
 'DT1WFO14':'412666ff325b72b66a7a60a50701bcde019357d9a6db991ac2bccaea85256b54',
 'DT1WFR13':'dedd0fe78d425bb826602373564964a91fe758338bca79ae2c703a2907f10345',
 'DT1WFR14':'fdebd341b5b569539b0bf72607d0018604f774b5074a2832e0e6e9206d93475f',
}
LABELS={150:'Total income  150',236:'Net income  236',260:'Taxable income  260',420:'Net federal tax  420',484:'Refund  484',485:'Balance owing  485'}
YEAR_REFS={2013:(b'F51E',b'on December 31, 2013?', 'c98e19c1f5ec6d773bf57e15c515365be83361878956975458f14703c3849e97'),2014:(b'A03E',b'spouse on December 31, 2014', '24391cb879decf73b77deb7185129db5b59f2c7fdab2cf25bc6e5ff41d5c9117')}
def sha(b):return hashlib.sha256(b).hexdigest()
def u32(b,p):return struct.unpack_from('<I',b,p)[0]
def kjd(name):
    raw=(BASE/name).read_bytes();assert sha(raw)==PINNED[name] and raw[:4]==b'KJD\0'
    start,size=struct.unpack_from('<II',raw,8);assert size%12==0 and start+size==len(raw)
    entries={}
    for p in range(start,start+size,12):
        key,offset,length=struct.unpack_from('<4sII',raw,p)
        assert offset+length<=start
        entries.setdefault(key,[]).append((p,offset,length))
    return raw,entries
def resource(raw,entries,key):
    assert len(entries[key])==1,'Selected resource name is ambiguous'
    directory,offset,length=entries[key][0];b=raw[offset:offset+length]
    return b,{'key':key.rstrip(b'\0').decode('ascii'),'directory_record_offset':directory,'offset':offset,'bytes':length,'sha256':sha(b)}
def form(b):
    assert b[:8]==b'DTFORM\0\0' and u32(b,204)==240 and u32(b,208)==len(b)-240
    version,flags,kind,length=struct.unpack_from('<IIII',b,240);assert (version,flags,kind)==(1,0,1)
    pool=b[256:256+length];assert len(pool)==length;p=256+length;segments=[];rows=[]
    while p<len(b):
        version,kind,size,count=struct.unpack_from('<IIII',b,p);assert version==1 and (kind,size) in [(1,12),(3,8),(7,28)]
        assert count>0 and p+16+size*count<=len(b)
        segments.append({'header_offset':p,'kind':kind,'record_bytes':size,'count':count})
        if kind==7:
            assert not rows
            rows=[(p+16+i*28,b[p+16+i*28:p+16+(i+1)*28]) for i in range(count)]
        p+=16+size*count
    assert p==len(b) and rows
    return pool,rows,segments
def fields(b):
    assert b[:8]==b'DTFIELD\0' and u32(b,212)==248 and u32(b,216)==len(b)-248 and u32(b,44)==24
    count=struct.unpack_from('<H',b,248)[0];assert 254+count*24==len(b)
    rows={}
    for i in range(count):
        at=254+i*24;row=struct.unpack_from('<IHHIIII',b,at);assert row[0] not in rows;rows[row[0]]=(at,row)
    return rows,count
report={'format':'dtmax-original-E11-form-bindings/v1','parser_sha256':sha(pathlib.Path(__file__).read_bytes()),'evidence_kind':'format-aware offline installed-artifact byte inspection','REA_native_artifact_session_opened':False,'source_client_data_read':False,'vendor_target_executed':False,'source_mutations':0,'hosted_mutations':0,'mapping_upgrades_applied':0,'years':[]}
for year in [2013,2014]:
    raw,idx=kjd('DT1WFO'+str(year)[-2:]);fr,fridx=kjd('DT1WFR'+str(year)[-2:]);meta,mref=resource(fr,fridx,b'E11\0')
    assert len(meta)==112 and meta[:4]==b'E11\0' and meta[12:64].split(b'\0')[0]==b'T1 comparative summary'
    e,er=resource(raw,idx,b'E11E');h,hr=resource(raw,idx,b'E11H');pool,objects,segments=form(e);fieldrows,count=fields(h)
    key,year_text,xml_sha=YEAR_REFS[year];yb,yref=resource(raw,idx,key)
    assert yb[:8]==b'DTFORM\0\0' and struct.unpack_from('<III',yb,240)==(1,0,1)
    ypool=yb[256:256+u32(yb,252)];assert len(ypool)==u32(yb,252) and ypool.count(year_text)>=1
    xpos=ypool.index(year_text);xml_name='DT1WAD'+str(year)[-2:]+'.XML';xml=(BASE/xml_name).read_bytes();assert sha(xml)==xml_sha
    xr=ET.fromstring(xml);assert xr.findtext('PRODUCT')=='DTMAXT1' and xr.findtext('YEAR')==str(year)
    year_support={'kind':'same_package_annual_form_instruction_and_explicit_XML_year','resource':yref,'current_tax_year_reference_offset':yref['offset']+256+xpos,'reference_bytes':len(year_text),'reference_sha256':sha(year_text),'XML_file':xml_name,'XML_sha256':xml_sha,'XML_product':xr.findtext('PRODUCT'),'XML_year':xr.findtext('YEAR'),'E11_uses_YEAR_placeholder':b'{YEAR}' in pool,'year_runtime_loader_binding_independently_proven':False}
    bound=[]
    for code,label in LABELS.items():
        labelbytes=label.encode('ascii');assert pool.count(labelbytes)==1;labelstart=pool.index(labelbytes);labelend=labelstart+len(labelbytes)
        # Static text records refer to this exact string, including its printed
        # three-digit line code. The following numeric object binds that row.
        labels=[(at,r)for at,r in objects if struct.unpack_from('<H',r,4)[0]==labelstart and struct.unpack_from('<H',r,6)[0]==len(labelbytes) and struct.unpack_from('<H',r,12)[0]==70]
        assert len(labels)==1;la,lr=labels[0];lx,ly=struct.unpack_from('<HH',lr)
        columns=[]
        for column in range(5):
            fid=code+column*1000
            amounts=[(at,r)for at,r in objects if struct.unpack_from('<H',r,14)[0]==1030 and u32(r,16)==fid]
            assert len(amounts)==1;at,r=amounts[0];xy=struct.unpack_from('<HH',r)
            hat,hrw=fieldrows[fid]
            assert xy==hrw[1:3] and xy[1]==ly and xy[0]>lx and u32(r,4)==labelend
            columns.append({'column':column,'serialized_field_id':fid,'coordinates':list(xy),'drawing_record_offset':er['offset']+at,'drawing_record_sha256':sha(r),'field_record_offset':hr['offset']+hat,'field_record_sha256':sha(h[hat:hat+24])})
        assert all(columns[i+1]['coordinates'][0]-columns[i]['coordinates'][0]==550 for i in range(4))
        bound.append({'requested_NXT_line':str(code*100),'legacy_printed_line':code,'source_form':'E11','production_column_field_id':code,'label_offset':er['offset']+256+labelstart,'label_bytes':len(labelbytes),'label_sha256':sha(labelbytes),'label_drawing_record_offset':er['offset']+la,'label_drawing_record_sha256':sha(lr),'columns':columns})
    report['years'].append({'year':year,'year_association':year_support,'packages':[{'name':name,'bytes':(BASE/name).stat().st_size,'sha256':PINNED[name]}for name in ['DT1WFO'+str(year)[-2:],'DT1WFR'+str(year)[-2:]]],'resources':[er,hr,mref],'drawing_segments':segments,'field_record_count':count,'field_identifiers_unique':True,'complete_segment_boundaries_verified':True,'six_label_binding_and_field_coordinate_matches':True,'bindings':bound})
report['conclusion']='The installed original 2013 and 2014 T1 E11 resources bind the six requested semantic rows to serialized production-column field IDs 150/236/260/420/484/485. This is stronger than printed labels alone: independent DTFORM numeric objects and DTFIELD records agree exactly on IDs and coordinates.'
report['limitations']=['The proprietary KJD container is unsupported by REA open_binary; this evidence is a format-aware offline byte parser, not vendor runtime observation.','REA process capture also rejected this native Windows host because it cannot verify descendant cleanup. No genuine REA success Evidence ID is claimed for the parser run.','DTFORM/DTFIELD layouts are derived from exact bounded structures and cross-file agreement; no claim is made to decode all property semantics.','The five columns are structurally established; the vendor runtime selector tuple (0,1,-1) is not independently decoded by this resource parser.','Year association is corroborated by a current-tax-year instruction in another form inside each same package, plus explicit DTMAXT1 XML YEAR metadata. E11 itself has a YEAR placeholder; no native year-loader call was analyzed.','The 2013 container has two duplicate unrelated directory names (WSAE/WSAF); selected E11E/E11H and year-reference resources are unique and unambiguous.','This establishes original form bindings, not complete extraction correctness, tax approval, return calculation or filing.','No pending hosted mapping state was changed.']
raw=json.dumps(report,indent=2).encode('utf-8')
if args.report:args.report.write_bytes(raw)
print(json.dumps({'report_sha256':sha(raw),'years':[r['year']for r in report['years']],'requested_line_bindings_verified_each_year':6,'matching_column_bindings_each_year':30,'source_client_data_read':False,'vendor_execution':False,'hosted_mutations':0}))
