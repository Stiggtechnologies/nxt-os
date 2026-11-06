"""Verify pinned 2005 DT Max E11 form bindings without reading client data."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

MAX_PACKAGE_BYTES = 64 * 1024 * 1024
PINNED = {
    'DT1WFO05': '90e36c34c9ad98691c872b6d0bc5482459ec5db84f9f32f86b926f234234f745',
    'DT1WFR05': '129d59cd40c9d3964f2eb481a28573fd7324043a955bca979e9f21df60062e41',
}
LABELS = {
    150: 'Total income', 236: 'Net income', 260: 'Taxable income',
    420: 'Net federal tax', 484: 'Refund', 485: 'Balance owing',
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def u16(data, offset):
    return struct.unpack_from('<H', data, offset)[0]


def u32(data, offset):
    return struct.unpack_from('<I', data, offset)[0]


def read_package(installation, name):
    path = installation / name
    require(path.is_file() and 28 <= path.stat().st_size <= MAX_PACKAGE_BYTES,
            'Missing or oversized pinned form package: ' + name)
    data = path.read_bytes()
    require(sha(data) == PINNED[name], 'Installed package hash differs: ' + name)
    require(data[:4] == b'KJD\0', 'Unsupported package header')
    start, length = struct.unpack_from('<II', data, 8)
    require(start >= 28 and length % 12 == 0 and start + length == len(data),
            'Invalid package directory bounds')
    entries = {}
    for at in range(start, start + length, 12):
        key, offset, size = struct.unpack_from('<4sII', data, at)
        require(offset + size <= start, 'Resource extends into the directory')
        entries.setdefault(key, []).append((at, offset, size))
    return data, entries


def resource(data, entries, key):
    matches = entries.get(key, [])
    require(len(matches) == 1, 'Missing or ambiguous selected resource')
    directory, start, length = matches[0]
    raw = data[start:start + length]
    return raw, {
        'key': key.rstrip(b'\0').decode('ascii'),
        'directory_record_offset': directory, 'offset': start,
        'bytes': length, 'sha256': sha(raw),
    }


def old_header(data):
    require(len(data) >= 240 and data[:8] == b'DTFIELD\0' and u32(data, 8) == 48,
            'Unsupported older field/form header')
    require(u32(data, 204) == 240 and u32(data, 208) == len(data) - 240,
            'Older field/form data length mismatch')


def text_pool(data):
    old_header(data)
    require(len(data) >= 256 and struct.unpack_from('<III', data, 240) == (1, 0, 1),
            'Unsupported text-pool header')
    length = u32(data, 252)
    require(256 + length <= len(data), 'Text pool exceeds resource')
    return data[256:256 + length]


def drawings(data, pool):
    at = 256 + len(pool)
    segments, rows = [], []
    while at < len(data):
        require(at + 16 <= len(data), 'Truncated drawing-segment header')
        version, kind, size, count = struct.unpack_from('<4I', data, at)
        require(version == 1 and (kind, size) in [(1, 12), (3, 8), (7, 16)]
                and count > 0 and at + 16 + size * count <= len(data),
                'Unsupported or unbounded drawing segment')
        segments.append({'header_offset': at, 'kind': kind,
                         'record_bytes': size, 'count': count})
        if kind == 7:
            require(not rows, 'Ambiguous numeric drawing segment')
            rows = [(at + 16 + i * size, data[at + 16 + i * size:at + 16 + (i + 1) * size])
                    for i in range(count)]
        at += 16 + size * count
    require(at == len(data) and len(rows) == 716, 'Unexpected 2005 drawing layout')
    return rows, segments


def field_records(data):
    old_header(data)
    require(len(data) >= 246 and u32(data, 44) == 24, 'Invalid field-record header')
    count = u16(data, 240)
    require(count == 520 and 246 + count * 24 == len(data),
            'Invalid older field-record array bounds')
    rows = {}
    for i in range(count):
        at = 246 + i * 24
        values = struct.unpack_from('<IHHIIII', data, at)
        require(values[0] not in rows, 'Duplicate serialized field identifier')
        rows[values[0]] = (at, values)
    return rows


def bounded_text(pool, record):
    start, length = u16(record, 4), u16(record, 6)
    require(start + length <= len(pool), 'Drawing text reference exceeds pool')
    return pool[start:start + length]


def verify(installation):
    package, directory = read_package(installation, 'DT1WFO05')
    registry, registry_directory = read_package(installation, 'DT1WFR05')
    display, display_ref = resource(package, directory, b'E11E')
    fields, fields_ref = resource(package, directory, b'E11H')
    meta, meta_ref = resource(registry, registry_directory, b'E11\0')
    require(len(meta) == 112 and meta[:4] == b'E11\0'
            and meta[12:64].split(b'\0')[0] == b'T1 - Summary',
            'Selected registry entry is not the expected E11 summary')
    pool = text_pool(display)
    objects, segments = drawings(display, pool)
    source_fields = field_records(fields)
    bindings = []
    for identifier, name in LABELS.items():
        label, line = name.encode('ascii'), str(identifier).encode('ascii')
        labels = [(at, row) for at, row in objects
                  if u16(row, 14) == 301 and bounded_text(pool, row) == label]
        require(len(labels) == 1, 'Missing or ambiguous semantic label')
        label_at, label_row = labels[0]
        label_start = u16(label_row, 4)
        label_x, label_y = struct.unpack_from('<HH', label_row)
        codes = [(at, row) for at, row in objects if u16(row, 12) == 70
                 and u16(row, 2) == label_y and bounded_text(pool, row) == line]
        require(len(codes) == 1, 'Missing or ambiguous printed line code')
        code_at, code_row = codes[0]
        code_start = u16(code_row, 4)
        text_end = max(label_start + len(label), code_start + len(line))
        columns = []
        for column in range(5):
            field_id = identifier + column * 1000
            require(field_id in source_fields, 'Expected source field is absent')
            field_at, field = source_fields[field_id]
            coordinates = field[1:3]
            numeric = [(at, row) for at, row in objects if u16(row, 14) == 1030
                       and struct.unpack_from('<HH', row) == coordinates]
            require(len(numeric) == 1, 'Numeric coordinate join is not unique')
            numeric_at, numeric_row = numeric[0]
            require(coordinates[1] == label_y and coordinates[0] > label_x
                    and u32(numeric_row, 4) == text_end
                    and field[3:6] == (224, 99, 65536),
                    'Semantic row, numeric drawing and field record do not agree')
            columns.append({
                'column': column, 'serialized_field_id': field_id,
                'coordinates': list(coordinates),
                'numeric_drawing_record_offset': display_ref['offset'] + numeric_at,
                'numeric_drawing_record_bytes': 16,
                'numeric_drawing_record_sha256': sha(numeric_row),
                'numeric_drawing_embeds_field_id': False,
                'field_record_offset': fields_ref['offset'] + field_at,
                'field_record_bytes': 24,
                'field_record_sha256': sha(fields[field_at:field_at + 24]),
                'join': 'unique_exact_coordinates_to_separate_DTFIELD_identifier',
            })
        require(all(columns[i + 1]['coordinates'][0] - columns[i]['coordinates'][0] == 550
                    for i in range(4)), 'Comparison columns have unexpected geometry')
        bindings.append({
            'requested_NXT_line': str(identifier * 100), 'source_form': 'E11',
            'production_column_field_id': identifier,
            'label_offset': display_ref['offset'] + 256 + label_start,
            'label_bytes': len(label), 'label_sha256': sha(label),
            'label_drawing_record_offset': display_ref['offset'] + label_at,
            'label_drawing_record_sha256': sha(label_row),
            'printed_legacy_line': identifier,
            'printed_line_offset': display_ref['offset'] + 256 + code_start,
            'printed_line_drawing_record_offset': display_ref['offset'] + code_at,
            'printed_line_drawing_record_sha256': sha(code_row), 'columns': columns,
        })
    annual, annual_ref = resource(package, directory, b'A89E')
    annual_pool = text_pool(annual)
    phrase = b'your 2005 income taxreturn'
    require(annual_pool.count(phrase) == 1, 'Annual year corroboration is ambiguous')
    return {
        'format': 'dtmax-2005-installed-E11-bindings/v1', 'year': 2005,
        'parser_sha256': sha(Path(__file__).read_bytes()),
        'evidence_kind': 'offline installed-artifact byte inspection',
        'packages': [{'name': name, 'sha256': PINNED[name],
                      'bytes': len(package if name == 'DT1WFO05' else registry)}
                     for name in PINNED],
        'resources': [display_ref, fields_ref, meta_ref],
        'drawing_segments': segments, 'field_record_count': len(source_fields),
        'complete_segment_boundaries_verified': True,
        'field_identifiers_unique': True, 'bindings': bindings,
        'year_association': {
            'kind': 'same_package_explicit_2005_income_tax_return_reference',
            'resource': annual_ref,
            'phrase_offset': annual_ref['offset'] + 256 + annual_pool.index(phrase),
            'phrase_bytes': len(phrase), 'phrase_sha256': sha(phrase),
            'DT1WAD05_XML_present': (installation / 'DT1WAD05.XML').is_file(),
            'XML_metadata_used': False,
            'year_runtime_loader_binding_independently_proven': False,
        },
        'source_client_data_read': False, 'vendor_code_executed': False,
        'source_mutations': 0, 'hosted_mutations': 0,
        'financial_values_approved': False,
        'limitations': [
            'Older 16-byte numeric drawings do not embed field IDs. Each is joined by an exact unique coordinate pair to a separate 24-byte DTFIELD identifier record.',
            'These derived layouts are fully bounded for the selected resources; unrelated drawing/property semantics are not decoded.',
            'No 2005 XML-year metadata is used. Year association comes from an explicit 2005 income-tax-return reference inside the same pinned package.',
            'The native year-loader and E11 selector tuple implementation are not independently verified.',
            'This script reads only installed form packages. It neither inspects client contexts nor establishes missing-versus-zero amount states.',
            'Binding verification supplies no financial amounts and is not tax approval, calculation, filing, or successful vendor runtime observation.',
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--installation', type=Path,
                        default=Path(r'C:\Program Files (x86)\DT Max'))
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if args.report:
        require(not args.report.resolve().is_relative_to(args.installation.resolve()),
                'Report output must be outside the DT Max installation')
        require(not args.report.exists(), 'Refusing to overwrite an existing report')
    report = verify(args.installation)
    raw = json.dumps(report, indent=2).encode('utf-8')
    if args.report:
        # Explicit report output only; installed files are never opened for writing.
        with args.report.open('xb') as destination:
            destination.write(raw)
    print(json.dumps({
        'report_sha256': sha(raw), 'year': 2005,
        'requested_line_bindings_verified': len(report['bindings']),
        'matching_column_bindings': sum(len(b['columns']) for b in report['bindings']),
        'numeric_drawings_embed_field_ids': False,
        'source_client_data_read': False, 'vendor_execution': False,
        'hosted_mutations': 0, 'financial_values_approved': False,
    }))


if __name__ == '__main__':
    main()
