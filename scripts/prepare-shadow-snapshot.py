#!/usr/bin/env python3
"""Prepare a NEW private parallel-test snapshot pack; no network or posting.

The original ZIP is preserved byte-for-byte. Only five financial report members
are parsed. All output, including company names and raw cells, is confidential.
"""
import argparse
import datetime as dt
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('qbo_shadow_source_contract', Path(__file__).with_name('inspect-qbo-export.py'))
qbo = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(qbo)

MAX_ARCHIVE = 4 * 1024 * 1024
MAX_CHUNK = 512 * 1024
MAX_TOTAL = 24 * 1024 * 1024
MAX_CHUNKS = 512
MAX_CELLS = 500_000
MAX_MANIFEST = 128 * 1024
MAX_PARTS = 128
MAX_EXCEPTIONS = 2000
SOURCE_TYPES = {'n', 's', 'inlineStr', 'str'}
MONETARY_COLUMNS = {'trial_balance': {'B', 'C'}, 'balance_sheet': {'B'}, 'profit_and_loss': {'B'}, 'journal': {'H', 'I'}, 'general_ledger': {'H', 'I', 'J'}}


class PreparationError(ValueError):
    pass


def require(condition, code):
    if not condition:
        raise PreparationError(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode('utf-8')


def utf8_bound(value, limit, code, nonempty=True):
    require(isinstance(value, str) and (bool(value) or not nonempty) and len(value.encode('utf-8')) <= limit, code)
    return value


def label(value, limit, code):
    utf8_bound(value, limit, code)
    require(not re.search(r'[\x00-\x1f\x7f]', value), code)
    return value


def column_number(column):
    value = 0
    for char in column:
        value = value * 26 + ord(char) - ord('A') + 1
    require(1 <= value <= 16384, 'invalid_excel_column')
    return value


def captured_at(value):
    require(isinstance(value, str) and re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?(?:Z|[+-]\d{2}:\d{2})', value), 'invalid_capture_timestamp')
    try:
        parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
        require(parsed.utcoffset() is not None, 'capture_timezone_required')
        return parsed.astimezone(dt.timezone.utc).isoformat().replace('+00:00', 'Z')
    except ValueError as exc:
        raise PreparationError('invalid_capture_timestamp') from exc


class FormulaView(qbo.Workbook):
    """Use the audited bounded cents evaluator without rejecting raw capture."""
    def __init__(self, cells):
        self.cells = cells
        self._evaluated, self._evaluating = {}, set()
        self._eval_work = 0
        self.formula_cache_mismatches, self.missing_formula_caches = [], []
        self.literal_formula_count = self.sum_formula_count = self.arithmetic_formula_count = 0

    def amount(self, address, blank_zero=True):
        cell = self.cells.get(address)
        if cell and cell['formula'] is not None and cell['compiled'] is None:
            raise qbo.AssessmentError('unsupported_formula_preserved')
        return super().amount(address, blank_zero)


def raw_report(data, kind):
    """Read raw logical cell values, including empty values and formula caches.

    Shared/inline strings are decoded to their logical text. Exact XML indexes,
    formatting and all original source bytes remain in the retained ZIP.
    """
    rows, cells, columns = [], {}, set()
    with qbo.archive(data) as archive:
        names = set(archive.namelist())
        book = qbo.xml(qbo.member(archive, 'xl/workbook.xml'))
        relations = qbo.xml(qbo.member(archive, 'xl/_rels/workbook.xml.rels'))
        relation_map = {}
        for relation in relations:
            require(relation.get('TargetMode', '').lower() != 'external', 'external_worksheet_relationship_not_followed')
            require(relation.get('Id') not in relation_map, 'duplicate_workbook_relationship')
            relation_map[relation.get('Id')] = relation
        sheets = book.findall(qbo.NS + 'sheets/' + qbo.NS + 'sheet')
        require(len(sheets) == 1 and sheets[0].get('state', 'visible') == 'visible', 'one_visible_sheet_required')
        sheet_name = utf8_bound(sheets[0].get('name', ''), 128, 'sheet_name_limit')
        relation = relation_map.get(sheets[0].get(qbo.RID))
        require(relation is not None and relation.get('Type', '').endswith('/worksheet'), 'worksheet_relationship_required')
        target = relation.get('Target', '')
        require(target.startswith('worksheets/') and '..' not in target and '\\' not in target, 'unsafe_worksheet_relationship')
        shared = []
        if 'xl/sharedStrings.xml' in names:
            strings = qbo.xml(qbo.member(archive, 'xl/sharedStrings.xml'))
            for item in strings.findall(qbo.NS + 'si'):
                require(len(shared) < 100_000, 'shared_string_count_limit')
                shared.append(utf8_bound(''.join(t.text or '' for t in item.iter(qbo.NS + 't')), 65_536, 'raw_cell_value_limit', False))
        sheet = qbo.xml(qbo.member(archive, 'xl/' + target))
        seen_rows = set()
        for row in sheet.findall(qbo.NS + 'sheetData/' + qbo.NS + 'row'):
            require(re.fullmatch(r'[1-9][0-9]{0,6}', row.get('r', '')) is not None, 'invalid_row_address')
            row_number = int(row.get('r'))
            require(1 <= row_number <= 1_048_576 and row_number not in seen_rows and len(seen_rows) < 100_000, 'duplicate_or_excessive_row')
            seen_rows.add(row_number)
            row_cells = []
            for c in row.findall(qbo.NS + 'c'):
                address = c.get('r', '')
                match = re.fullmatch(r'([A-Z]{1,3})([1-9][0-9]{0,6})', address)
                require(match is not None and int(match[2]) == row_number and address not in cells, 'invalid_or_duplicate_cell_address')
                column = match[1]
                column_number(column)
                ctype = c.get('t', 'n')
                require(ctype in SOURCE_TYPES, 'source_type_not_supported_in_shadow_contract')
                require(len(c.findall(qbo.NS + 'f')) <= 1 and len(c.findall(qbo.NS + 'v')) <= 1 and len(c.findall(qbo.NS + 'is')) <= 1, 'duplicate_cell_representation')
                require(all(child.tag in (qbo.NS + 'f', qbo.NS + 'v', qbo.NS + 'is') for child in c), 'unsupported_cell_child')
                f, v, inline = c.find(qbo.NS + 'f'), c.find(qbo.NS + 'v'), c.find(qbo.NS + 'is')
                require((ctype == 'inlineStr' and inline is not None and v is None and f is None) or (ctype != 'inlineStr' and inline is None), 'mixed_cell_representations')
                value = '' if v is None else v.text or ''
                if ctype == 's':
                    require(re.fullmatch(r'[0-9]{1,8}', value) is not None and int(value) < len(shared), 'invalid_shared_string_index')
                    value = shared[int(value)]
                elif ctype == 'inlineStr':
                    value = ''.join(t.text or '' for t in c.iter(qbo.NS + 't'))
                utf8_bound(value, 65_536, 'raw_cell_value_limit', False)
                formula = None if f is None else utf8_bound(f.text or '', 16_384, 'raw_formula_limit', False)
                compiled, formula_problem = None, None
                if formula is not None:
                    if f.attrib:
                        formula_problem = 'formula_attributes_not_interpreted'
                    elif ctype != 'n':
                        formula_problem = 'nonnumeric_formula_not_interpreted'
                    else:
                        try:
                            compiled = qbo.compile_formula(formula)
                        except qbo.AssessmentError as exc:
                            formula_problem = str(exc)
                entry = {'type': ctype, 'value': value, 'formula': formula, 'compiled': compiled, 'formula_problem': formula_problem}
                cells[address] = entry
                row_cells.append([column, ctype, value, formula, None])
                columns.add(column)
                require(len(cells) <= MAX_CELLS, 'report_cell_count_limit')
            row_cells.sort(key=lambda c: column_number(c[0]))
            rows.append([row_number, row_cells])
    rows.sort(key=lambda r: r[0])
    return {'id': kind, 'sheet_name': sheet_name, 'rows': rows, 'cells': cells, 'columns': sorted(columns, key=column_number), 'source_sha256': sha(data), 'source_bytes': len(data)}


def interpret_formulas(report):
    exceptions, evaluated, mismatches = [], 0, 0
    view = FormulaView(report['cells'])
    for row, row_cells in report['rows']:
        by_column = {cell[0]: cell for cell in row_cells}
        if report['id'] == 'journal' and row > 5:
            account = by_column.get('G')
            blank = lambda col: col not in by_column or by_column[col][2] == '' and by_column[col][3] is None
            if account and account[2].strip() and blank('H') and blank('I'):
                exceptions.append({'report_id': report['id'], 'row': row, 'code': 'source_blank_debit_credit', 'note': 'Source debit and credit cells are blank and remain blank. No zero, void or posting classification is inferred.'})
        for cell in row_cells:
            column, ctype, raw_value, formula, _ = cell
            if formula is None:
                continue
            address = f'{column}{row}'
            problem = report['cells'][address]['formula_problem']
            monetary_profile = column in MONETARY_COLUMNS[report['id']] and row > 5 and ctype == 'n'
            if problem is None and monetary_profile:
                try:
                    result = view.amount(address)
                    cell[4] = str(result)
                    evaluated += 1
                    if address in view.formula_cache_mismatches:
                        mismatches += 1
                except qbo.AssessmentError as exc:
                    problem = str(exc)
            elif problem is None:
                problem = 'outside_supported_monetary_formula_profile'
            if problem is not None:
                exceptions.append({'report_id': report['id'], 'row': row, 'code': f'formula_not_interpreted_{column}', 'note': f'Cell {column}: {problem}. Source formula and cache are preserved; no derived cents are supplied.'})
    return exceptions, evaluated, mismatches


class Chunks:
    def __init__(self):
        self.contents, self.total = {}, 0

    def add(self, content):
        require(isinstance(content, bytes) and 0 < len(content) <= MAX_CHUNK, 'chunk_byte_limit')
        content.decode('utf-8', errors='strict')
        key = sha(content)
        if key not in self.contents:
            require(len(self.contents) < MAX_CHUNKS and self.total + len(content) <= MAX_TOTAL, 'snapshot_content_budget')
            self.contents[key] = content
            self.total += len(content)
        else:
            require(self.contents[key] == content, 'chunk_hash_collision')
        return key


def row_parts(report, cohort_ref, archive_sha, chunks):
    prefix_object = {'format': 'nxt-shadow-report-rows/v1', 'cohort_ref': cohort_ref, 'source_archive_sha256': archive_sha, 'source_report_sha256': report['source_sha256'], 'report_id': report['id']}
    prefix = encoded(prefix_object)[:-1] + b',"rows":['
    suffix = b']}'
    pending, numbers, size, parts = [], [], len(prefix) + len(suffix), []

    def flush():
        nonlocal pending, numbers, size
        if pending:
            payload = prefix + b','.join(pending) + suffix
            key = chunks.add(payload)
            parts.append({'sha256': key, 'row_start': numbers[0], 'row_end': numbers[-1], 'row_count': len(numbers)})
            require(len(parts) <= MAX_PARTS, 'report_part_count_limit')
            pending, numbers, size = [], [], len(prefix) + len(suffix)

    for row in report['rows']:
        data = encoded(row)
        required = len(data) + (1 if pending else 0)
        if size + required > MAX_CHUNK:
            flush()
            required = len(data)
        require(size + required <= MAX_CHUNK, 'single_row_exceeds_chunk_limit')
        pending.append(data)
        numbers.append(row[0])
        size += required
    flush()
    return parts


def prepare_pack(source_bytes, *, cohort_ref, company_name, captured, cutoff_date, reviewer_label):
    require(0 < len(source_bytes) <= MAX_ARCHIVE, 'original_archive_byte_limit')
    utf8_bound(cohort_ref, 128, 'cohort_reference_limit')
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,63}', cohort_ref) is not None, 'opaque_cohort_reference_required')
    label(company_name, 240, 'company_name_limit')
    label(reviewer_label, 240, 'reviewer_label_limit')
    timestamp = captured_at(captured)
    try:
        require(re.fullmatch(r'\d{4}-\d{2}-\d{2}', cutoff_date) is not None and dt.date.fromisoformat(cutoff_date).isoformat() == cutoff_date, 'invalid_cutoff_date')
    except (ValueError, TypeError) as exc:
        raise PreparationError('invalid_cutoff_date') from exc
    archive_sha = sha(source_bytes)
    chunks = Chunks()
    # Raw hex text is split at byte boundaries: concatenation decodes to the
    # exact original ZIP, including members that were not parsed or inspected.
    archive_chunks = [chunks.add(source_bytes[at:at + MAX_CHUNK // 2].hex().encode('ascii')) for at in range(0, len(source_bytes), MAX_CHUNK // 2)]
    reports, exceptions, total_cells, formula_count, mismatch_count = [], [], 0, 0, 0
    with qbo.archive(source_bytes) as archive:
        selected = {}
        for item in archive.infolist():
            kind = qbo.classify(item.filename)
            if kind:
                require(kind not in selected, 'duplicate_financial_report_kind')
                selected[kind] = item.filename
        require('trial_balance' in selected and 'journal' in selected, 'trial_balance_and_journal_required')
        for kind in sorted(selected):
            report = raw_report(qbo.member(archive, selected[kind]), kind)
            company = report['cells'].get('A1')
            title = report['cells'].get('A2')
            require(company is not None and company['formula'] is None and company['type'] in ('s', 'str', 'inlineStr') and company['value'].strip() == company_name, 'source_company_heading_mismatch')
            require(title is not None and title['formula'] is None and title['value'].strip().lower() == qbo.TITLES[kind], 'source_report_title_mismatch')
            total_cells += len(report['cells'])
            require(total_cells <= MAX_CELLS, 'snapshot_aggregate_cell_limit')
            findings, interpreted, mismatched = interpret_formulas(report)
            exceptions.extend(findings)
            require(len(exceptions) <= MAX_EXCEPTIONS, 'exception_count_limit')
            formula_count += interpreted
            mismatch_count += mismatched
            reports.append({'id': kind, 'title': qbo.TITLES[kind].title(), 'sheet_name': report['sheet_name'], 'source_sha256': report['source_sha256'], 'source_bytes': report['source_bytes'], 'columns': report['columns'], 'row_count': len(report['rows']), 'parts': row_parts(report, cohort_ref, archive_sha, chunks)})
    manifest = {'format': 'nxt-shadow-snapshot/v1', 'id': f'qbo-{cohort_ref}-{archive_sha[:16]}', 'cohort_ref': cohort_ref, 'company_name': company_name, 'source_system': 'quickbooks', 'captured_at': timestamp, 'cutoff_date': cutoff_date, 'reviewer_label': reviewer_label, 'source_of_truth': 'original_system', 'mode': 'parallel_test', 'source_archive': {'file_name': 'quickbooks-export.zip', 'sha256': archive_sha, 'bytes': len(source_bytes), 'encoding': 'hex', 'chunks': archive_chunks}, 'reports': reports, 'exceptions': exceptions, 'limitations': [
        'Parallel test only. The original system remains the source of truth; this snapshot does not post, import, alter balances, approve a cutover or replace certified tax filing.',
        'The original ZIP is preserved byte-for-byte, including uninspected members. Only selected financial report contents were parsed; customer, employee and supplier list contents were not opened.',
        'Rows preserve source worksheet indices and logical shared/inline text, numeric raw strings, formula expressions and cached values. Formatting, source XML indexes and unsupported metadata remain in the exact original archive.',
        'The server validates snapshot hashes and structural bindings but does not independently parse XLSX to attest that displayed rows equal the archived workbook. Preparation evidence and source review are separate requirements.',
        f'{formula_count} supported numeric monetary formulas were interpreted with bounded exact-cent arithmetic; {mismatch_count} differed from their numeric caches. Source formulas and caches remain unchanged. Ordinary numeric/date cells are not converted to money.',
        'Blank amounts remain blank and are not inferred to be zero, void or non-posting. Any unsupported formula remains raw with no interpreted cents and an explicit exception.',
        'Requested cutoff is a label, not a filter or source freeze. All supplied report rows are retained as-is; completeness, common source cutoff and legal company identity are not independently verified.',
        'Capture time is acquisition metadata and does not independently prove the source-export moment or a source-system freeze.',
        'CAD is an unverified source-currency assumption. Full source history, archives, attachments, open items and durable source account/transaction identity are not established by these reports.',
        'Reviewer label is supplied metadata, not verified individual identity or independent approval. Current shared-workspace authorization is not company tenancy or segregation of duties.',
        'Recovery, retention, independent key custody, financial reconciliation and cohort cutover gates remain separate. Preserving report discrepancies in a parallel test does not close these gates.',
    ]}
    require(len(encoded(manifest)) <= MAX_MANIFEST, 'manifest_byte_limit')
    receipt = {'format': 'nxt-shadow-preparation/v1', 'manifest_sha256': sha(encoded(manifest)), 'source_sha256': archive_sha, 'source_bytes': len(source_bytes), 'unique_chunks': len(chunks.contents), 'unique_content_bytes': chunks.total, 'aggregate_cells': total_cells, 'reports': [{'id': r['id'], 'rows': r['row_count'], 'parts': len(r['parts'])} for r in reports], 'exceptions': len(exceptions), 'target_uploads': 0, 'production_writes': 0, 'network_requests': 0, 'mode': 'parallel_test', 'source_of_truth': 'original_system'}
    return manifest, chunks.contents, receipt


def write_pack(output, manifest, chunks, receipt):
    output.mkdir(mode=0o777 if os.name == 'nt' else 0o700)
    chunk_dir = output / 'chunks'
    chunk_dir.mkdir()
    for key, content in chunks.items():
        require(re.fullmatch(r'[a-f0-9]{64}', key) is not None and sha(content) == key and len(content) <= MAX_CHUNK, 'invalid_chunk_identity')
        with (chunk_dir / f'{key}.txt').open('xb') as target:
            target.write(content)
    with (output / 'preparation.json').open('xb') as target:
        target.write(encoded(receipt))
    # Manifest is the completion marker and exact hashed bytes, written last.
    with (output / 'manifest.json').open('xb') as target:
        target.write(encoded(manifest))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True, help='NEW private pack directory; parent must exist')
    parser.add_argument('--cohort', required=True)
    parser.add_argument('--company', required=True)
    parser.add_argument('--captured-at', help='RFC3339 acquisition time; default observed local source-file modification time')
    parser.add_argument('--cutoff-date', required=True)
    parser.add_argument('--reviewer', required=True)
    args = parser.parse_args(argv)
    try:
        source = qbo.read_source(args.source)
        timestamp = args.captured_at or dt.datetime.fromtimestamp(args.source.stat().st_mtime, dt.timezone.utc).isoformat().replace('+00:00', 'Z')
        manifest, chunks, receipt = prepare_pack(source, cohort_ref=args.cohort, company_name=args.company, captured=timestamp, cutoff_date=args.cutoff_date, reviewer_label=args.reviewer)
        if not args.captured_at:
            manifest['limitations'].append('Capture time is the observed local acquisition file modification time, not an independently witnessed source-export timestamp or freeze.')
            require(len(encoded(manifest)) <= MAX_MANIFEST, 'manifest_byte_limit')
            receipt['manifest_sha256'] = sha(encoded(manifest))
        write_pack(args.out, manifest, chunks, receipt)
        print(f"Parallel-test pack prepared; reports={len(manifest['reports'])}; chunks={receipt['unique_chunks']}; content_bytes={receipt['unique_content_bytes']}; exceptions={receipt['exceptions']}; target_uploads=0; production_writes=0")
        return 0
    except (PreparationError, qbo.AssessmentError, OSError, UnicodeError) as exc:
        code = str(exc) if isinstance(exc, (PreparationError, qbo.AssessmentError)) else type(exc).__name__
        print(f'Local shadow preparation failed ({code}); no target upload or posting performed.', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
