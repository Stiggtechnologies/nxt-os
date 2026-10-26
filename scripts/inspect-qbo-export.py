#!/usr/bin/env python3
"""Read-only, bounded assessment of selected QBO financial report XLSX files.

No source program, Excel engine, external formula, importer or network is used.
Reports can contain private account labels and amounts: keep output private.
"""
import argparse
import datetime as dt
from decimal import Decimal, InvalidOperation, localcontext
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import zipfile
import xml.etree.ElementTree as ET

MAX_SOURCE = 64 * 1024 * 1024
MAX_MEMBER = 32 * 1024 * 1024
MAX_EXPANDED = 128 * 1024 * 1024
MAX_ENTRIES = 512
MAX_RATIO = 200
MAX_CELLS = 500_000
MAX_ROWS = 100_000
MAX_MONEY = 9_007_199_254_740_991
MAX_EVAL_WORK = 1_000_000
MAX_DEPENDENCY_DEPTH = 64
MAX_FORMULA_NESTING = 256
NS = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
REL = '{http://schemas.openxmlformats.org/package/2006/relationships}'
RID = '{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id'
KINDS = {'trialbalance': 'trial_balance', 'journal': 'journal', 'generalledger': 'general_ledger', 'balancesheet': 'balance_sheet', 'profitandloss': 'profit_and_loss'}
TITLES = {'trial_balance': 'trial balance', 'journal': 'journal', 'general_ledger': 'general ledger', 'balance_sheet': 'balance sheet', 'profit_and_loss': 'profit and loss'}
NUMBER = re.compile(r'[+-]?(?:\d+)(?:\.\d+)?\Z')
ADDRESS = re.compile(r'([A-Z]{1,3})([1-9]\d{0,5})\Z')
INSPECTOR_SCRIPT_SHA256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


class AssessmentError(ValueError):
    def __init__(self, code, location=None):
        super().__init__(code)
        self.location = location


def require(condition, code, location=None):
    if not condition:
        raise AssessmentError(code, location)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode('utf-8')


def archive(data):
    require(len(data) <= MAX_SOURCE, 'archive_byte_limit')
    try:
        z = zipfile.ZipFile(io.BytesIO(data))
        entries = z.infolist()
        require(0 < len(entries) <= MAX_ENTRIES, 'archive_entry_limit')
        names, expanded = set(), 0
        for item in entries:
            name = item.filename
            parts = name.rstrip('/').split('/')
            require(name and not name.startswith('/') and '\\' not in name and ':' not in name and all(p not in ('', '.', '..') for p in parts), 'unsafe_archive_path')
            key = name.casefold()
            require(key not in names, 'duplicate_archive_path')
            names.add(key)
            require(not item.flag_bits & 1, 'encrypted_archive_member')
            require(not stat.S_ISLNK(item.external_attr >> 16), 'linked_archive_member')
            require(item.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED), 'unsupported_archive_compression')
            require(item.file_size <= MAX_MEMBER, 'archive_member_byte_limit')
            require(item.file_size <= max(1, item.compress_size) * MAX_RATIO, 'archive_expansion_ratio')
            expanded += item.file_size
            require(expanded <= MAX_EXPANDED, 'archive_expanded_byte_limit')
        return z
    except (zipfile.BadZipFile, OSError) as exc:
        raise AssessmentError('invalid_archive') from exc


def member(z, name):
    try:
        info = z.getinfo(name)
        require(info.file_size <= MAX_MEMBER, 'archive_member_byte_limit')
        with z.open(info) as source:
            data = source.read(MAX_MEMBER + 1)
        require(len(data) == info.file_size and len(data) <= MAX_MEMBER, 'member_size_mismatch')
        return data
    except (KeyError, zipfile.BadZipFile, OSError, RuntimeError) as exc:
        raise AssessmentError('unreadable_archive_member') from exc


def xml(data):
    require(len(data) <= MAX_MEMBER, 'xml_byte_limit')
    require(b'\0' not in data, 'only_utf8_xml_supported')
    try:
        decoded = data.decode('utf-8-sig', errors='strict')
    except UnicodeDecodeError as exc:
        raise AssessmentError('only_utf8_xml_supported') from exc
    declaration = re.match(r'\s*<\?xml[^?]*encoding\s*=\s*[\'\"]([^\'\"]+)', decoded, re.I)
    require(declaration is None or declaration[1].lower().replace('-', '') == 'utf8', 'only_utf8_xml_supported')
    require(not re.search(r'<!\s*(?:DOCTYPE|ENTITY)\b', decoded, re.I), 'xml_dtd_or_entity_forbidden')
    try:
        root = ET.fromstring(data)
    except ET.ParseError as exc:
        raise AssessmentError('invalid_xml') from exc
    stack, count = [(root, 0)], 0
    while stack:
        element, depth = stack.pop()
        count += 1
        require(count <= 2_000_000 and depth <= 32, 'xml_complexity_limit')
        stack.extend((child, depth + 1) for child in element)
    return root


def numeric(value):
    require(isinstance(value, str) and len(value) <= 64 and NUMBER.fullmatch(value) is not None, 'unsupported_numeric_token')
    try:
        with localcontext() as ctx:
            ctx.prec = 80
            amount = Decimal(value)
            cents = amount * 100
            require(cents == cents.to_integral_value(), 'fractional_cent')
            require(abs(cents) <= MAX_MONEY, 'amount_outside_native_safe_range')
            return int(cents)
    except InvalidOperation as exc:
        raise AssessmentError('invalid_decimal') from exc


def compile_formula(formula):
    """Iterative restricted arithmetic -> RPN; no Python/Excel execution."""
    require(isinstance(formula, str) and 0 < len(formula) <= 4096, 'formula_length_limit')
    output, operators, at, depth, tokens, expect_value = [], [], 0, 0, 0, True
    while at < len(formula):
        if formula[at] in ' \t\r\n':
            at += 1
            continue
        tokens += 1
        require(tokens <= 2048, 'formula_token_limit')
        part = formula[at:]
        sum_match = re.match(r'SUM\(([A-Z]{1,3})([1-9][0-9]{0,5}):\1([1-9][0-9]{0,5})\)', part, re.I)
        cell_match = re.match(r'[A-Z]{1,3}[1-9][0-9]{0,5}', part, re.I)
        number_match = re.match(r'[0-9]+(?:\.[0-9]+)?', part)
        if sum_match or cell_match or number_match:
            require(expect_value, 'unsupported_formula')
            if sum_match:
                first, last = int(sum_match[2]), int(sum_match[3])
                require(first <= last <= MAX_ROWS, 'formula_range_limit')
                output.append(('range', sum_match[1].upper(), first, last))
                at += len(sum_match[0])
            elif cell_match:
                require(int(re.search(r'[0-9]+', cell_match[0])[0]) <= MAX_ROWS, 'formula_range_limit')
                output.append(('cell', cell_match[0].upper()))
                at += len(cell_match[0])
            else:
                output.append(('literal', numeric(number_match[0])))
                at += len(number_match[0])
            expect_value = False
        elif formula[at] == '(':
            require(expect_value, 'unsupported_formula')
            operators.append('(')
            depth += 1
            # Observed exports use deeply left-nested addition totals. This is
            # an explicit bounded operator stack, never Python recursion.
            require(depth <= MAX_FORMULA_NESTING, 'formula_nesting_limit')
            at += 1
        elif formula[at] == ')':
            require(not expect_value and depth > 0, 'unsupported_formula')
            while operators and operators[-1] != '(':
                output.append(('op', operators.pop()))
            require(operators and operators.pop() == '(', 'unsupported_formula')
            depth -= 1
            while operators and operators[-1] in ('u+', 'u-'):
                output.append(('op', operators.pop()))
            at += 1
        elif formula[at] in '+-':
            operator = formula[at]
            if expect_value:
                operators.append('u' + operator)
            else:
                while operators and operators[-1] != '(':
                    output.append(('op', operators.pop()))
                operators.append(operator)
                expect_value = True
            at += 1
        else:
            raise AssessmentError('unsupported_formula')
    require(not expect_value and depth == 0, 'unsupported_formula')
    while operators:
        output.append(('op', operators.pop()))
    return output


class Workbook:
    def __init__(self, data, kind):
        self.kind, self.raw_hash, self.raw_bytes = kind, sha(data), len(data)
        self.cells, self.rows, self.formula_cache_mismatches, self.missing_formula_caches = {}, {}, [], []
        self.formula_count, self.literal_formula_count, self.sum_formula_count = 0, 0, 0
        self.arithmetic_formula_count, self._eval_work = 0, 0
        self._evaluated, self._evaluating = {}, set()
        with archive(data) as z:
            names = set(z.namelist())
            require(not any('externallinks/' in n.lower() or n.lower().endswith('vbaproject.bin') for n in names), 'external_or_macro_workbook_unsupported')
            book = xml(member(z, 'xl/workbook.xml'))
            props = book.find(NS + 'workbookPr')
            self.date1904 = props is not None and props.get('date1904', '0') in ('1', 'true')
            relations = xml(member(z, 'xl/_rels/workbook.xml.rels'))
            relation_map = {}
            for r in relations:
                require(r.get('TargetMode', '').lower() != 'external', 'external_workbook_relationship')
                require(r.get('Id') not in relation_map, 'duplicate_workbook_relationship')
                relation_map[r.get('Id')] = r
            sheets = book.findall(NS + 'sheets/' + NS + 'sheet')
            require(len(sheets) == 1 and sheets[0].get('state', 'visible') == 'visible', 'one_visible_sheet_required')
            relation = relation_map.get(sheets[0].get(RID))
            require(relation is not None and relation.get('Type', '').endswith('/worksheet'), 'worksheet_relationship_required')
            target = relation.get('Target', '')
            require(target.startswith('worksheets/') and '..' not in target and '\\' not in target, 'unsafe_worksheet_relationship')
            shared = []
            if 'xl/sharedStrings.xml' in names:
                strings = xml(member(z, 'xl/sharedStrings.xml'))
                for entry in strings.findall(NS + 'si'):
                    value = ''.join(t.text or '' for t in entry.iter(NS + 't'))
                    require(len(shared) < 100_000 and len(value) <= 65_536, 'shared_string_limit')
                    shared.append(value)
            sheet = xml(member(z, 'xl/' + target))
            seen_rows = set()
            for row in sheet.findall(NS + 'sheetData/' + NS + 'row'):
                require(re.fullmatch(r'[1-9]\d{0,5}', row.get('r', '')) is not None, 'invalid_row_address')
                number = int(row.get('r'))
                require(number <= MAX_ROWS and number not in seen_rows, 'duplicate_or_excessive_row')
                seen_rows.add(number)
                for c in row.findall(NS + 'c'):
                    address = c.get('r', '')
                    match = ADDRESS.fullmatch(address)
                    require(match is not None and int(match[2]) == number and address not in self.cells, 'invalid_or_duplicate_cell_address')
                    require(len(self.cells) < MAX_CELLS, 'cell_limit')
                    require(c.get('t', 'n') in ('n', 's', 'inlineStr', 'str'), 'unsupported_or_error_cell_type')
                    require(len(c.findall(NS + 'f')) <= 1 and len(c.findall(NS + 'v')) <= 1 and len(c.findall(NS + 'is')) <= 1, 'duplicate_cell_value_or_formula')
                    require(all(child.tag in (NS + 'f', NS + 'v', NS + 'is') for child in c), 'unsupported_cell_child')
                    f, v, inline = c.find(NS + 'f'), c.find(NS + 'v'), c.find(NS + 'is')
                    formula = None if f is None else f.text or ''
                    require(f is None or not f.attrib, 'shared_array_formula_unsupported')
                    compiled = None if f is None else compile_formula(formula)
                    ctype = c.get('t', 'n')
                    require((ctype == 'inlineStr' and inline is not None and v is None and f is None) or (ctype != 'inlineStr' and inline is None), 'mixed_cell_representations')
                    require(f is None or ctype == 'n', 'numeric_formula_cell_required')
                    value = '' if v is None else v.text or ''
                    if ctype == 's':
                        require(value.isdigit() and int(value) < len(shared), 'invalid_shared_string_index')
                        value = shared[int(value)]
                    elif ctype == 'inlineStr':
                        value = ''.join(t.text or '' for t in c.iter(NS + 't'))
                    require(len(value) <= 65_536, 'cell_text_limit')
                    self.cells[address] = {'type': ctype, 'value': value, 'formula': formula, 'compiled': compiled}
                    self.rows.setdefault(number, {})[match[1]] = address
                    if formula is not None:
                        self.formula_count += 1
        require(self.text('A1') != '', 'company_heading_required')
        require(self.text('A2').strip().lower() == TITLES[kind], 'report_title_mismatch')
        self.company_hash = sha(self.text('A1').encode('utf-8'))
        self.period_label = self.text('A3')
        require(self.period_label != '', 'report_period_required')

    def text(self, address):
        c = self.cells.get(address)
        if not c:
            return ''
        require(c['formula'] is None and c['type'] in ('s', 'inlineStr', 'str'), 'text_cell_required')
        return c['value'].strip()

    def amount(self, address, blank_zero=True):
        self._eval_work += 1
        require(self._eval_work <= MAX_EVAL_WORK, 'formula_evaluation_work_limit')
        if address in self._evaluated:
            return self._evaluated[address]
        c = self.cells.get(address)
        if not c or c['value'] == '' and c['formula'] is None:
            require(blank_zero, 'amount_required')
            return 0
        require(c['type'] == 'n', 'numeric_cell_required')
        require(address not in self._evaluating, 'circular_formula')
        require(len(self._evaluating) < MAX_DEPENDENCY_DEPTH, 'formula_dependency_depth_limit')
        self._evaluating.add(address)
        try:
            formula = c['formula']
            if formula is None:
                result = numeric(c['value'])
            else:
                values = []
                for token in c['compiled']:
                    self._eval_work += 1
                    require(self._eval_work <= MAX_EVAL_WORK, 'formula_evaluation_work_limit')
                    if token[0] == 'literal':
                        values.append(token[1])
                    elif token[0] == 'cell':
                        values.append(self.amount(token[1]))
                    elif token[0] == 'range':
                        require(self._eval_work + token[3] - token[2] + 1 <= MAX_EVAL_WORK, 'formula_evaluation_work_limit')
                        values.append(sum(self.amount(f'{token[1]}{r}') for r in range(token[2], token[3] + 1)))
                    else:
                        op = token[1]
                        require(len(values) >= (1 if op.startswith('u') else 2), 'invalid_formula_operands')
                        if op.startswith('u'):
                            values.append(values.pop() * (-1 if op == 'u-' else 1))
                        else:
                            right, left = values.pop(), values.pop()
                            values.append(left + right if op == '+' else left - right)
                    require(not values or abs(values[-1]) <= MAX_MONEY, 'amount_outside_native_safe_range')
                require(len(values) == 1, 'invalid_formula_operands')
                result = values[0]
                if NUMBER.fullmatch(formula):
                    self.literal_formula_count += 1
                elif len(c['compiled']) == 1 and c['compiled'][0][0] == 'range':
                    self.sum_formula_count += 1
                else:
                    self.arithmetic_formula_count += 1
            if formula is not None:
                if c['value'] == '':
                    self.missing_formula_caches.append(address)
                elif numeric(c['value']) != result:
                    self.formula_cache_mismatches.append(address)
            self._evaluated[address] = result
            return result
        finally:
            self._evaluating.remove(address)

    def metadata(self):
        populated = sum(any(self.cells[a]['value'] != '' or self.cells[a]['formula'] is not None for a in cols.values()) for cols in self.rows.values())
        return {'report': self.kind, 'bytes': self.raw_bytes, 'sha256': self.raw_hash, 'populated_rows': populated, 'cell_slots': len(self.cells), 'formula_count': self.formula_count, 'company_heading_sha256': self.company_hash, 'period_label': self.period_label, 'semantics': 'metadata_only'}


def nonempty_columns(book, row):
    return {col for col, address in book.rows.get(row, {}).items() if book.cells[address]['value'] != '' or book.cells[address]['formula'] is not None}


def trial_balance(book):
    require(book.text('B5') == 'Debit' and book.text('C5') == 'Credit', 'tb_header_mismatch')
    accounts, seen, total_row, footer_count = [], set(), None, 0
    for row in sorted(r for r in book.rows if r > 5):
        cols = nonempty_columns(book, row)
        if not cols:
            continue
        label = book.text(f'A{row}') if 'A' in cols else ''
        require(cols <= {'A', 'B', 'C'}, 'unexpected_tb_columns')
        if total_row is not None:
            require(cols == {'A'}, 'financial_rows_after_tb_total')
            footer_count += 1
            continue
        debit, credit = book.amount(f'B{row}'), book.amount(f'C{row}')
        if label.upper() == 'TOTAL':
            require('B' in cols and 'C' in cols, 'tb_total_amounts_required')
            total_row = row
            reported = (debit, credit)
            continue
        require(label and bool(cols & {'B', 'C'}), 'tb_account_amount_required')
        require(label not in seen, 'duplicate_tb_account_label')
        require(not (debit and credit), 'tb_account_has_both_sides')
        seen.add(label)
        accounts.append({'label': label, 'label_ref': sha(label.encode('utf-8')), 'row': row, 'debit_cents': str(debit), 'credit_cents': str(credit), 'net_debit_cents': str(debit - credit)})
    require(accounts and total_row is not None, 'tb_accounts_and_total_required')
    totals = (sum(int(a['debit_cents']) for a in accounts), sum(int(a['credit_cents']) for a in accounts))
    require(totals == reported, 'tb_total_mismatch')
    require(totals[0] == totals[1], 'unbalanced_trial_balance')
    return {'status': 'supported_controls_passed', 'account_count': len(accounts), 'accounts': accounts, 'total_row': total_row, 'debit_cents': str(totals[0]), 'credit_cents': str(totals[1]), 'footer_rows': footer_count, 'formula_cache_mismatch_count': len(book.formula_cache_mismatches), 'formula_cache_mismatch_cells': book.formula_cache_mismatches, 'missing_formula_cache_count': len(book.missing_formula_caches), 'literal_formulas_evaluated': book.literal_formula_count, 'sum_formulas_evaluated': book.sum_formula_count, 'arithmetic_formulas_evaluated': book.arithmetic_formula_count, 'identity_basis': 'Exact account label hash and source row, not durable source account IDs.'}


def date_order(book, choice):
    require(choice in ('auto', 'dmy', 'mdy'), 'invalid_date_order')
    evidence = set()
    for row in book.rows:
        if row <= 5 or 'B' not in nonempty_columns(book, row):
            continue
        c = book.cells[f'B{row}']
        if c['type'] not in ('s', 'inlineStr', 'str'):
            continue
        v = book.text(f'B{row}')
        match = re.fullmatch(r'(\d{1,2})/(\d{1,2})/(\d{4})', v)
        if match:
            first, second = int(match[1]), int(match[2])
            if first > 12:
                evidence.add('dmy')
            if second > 12:
                evidence.add('mdy')
    require(len(evidence) <= 1, 'inconsistent_journal_date_order')
    if choice != 'auto':
        require(not evidence or choice in evidence, 'journal_date_order_conflict')
        return choice, 'operator_supplied'
    return (next(iter(evidence)), 'unambiguous_source_dates') if evidence else (None, 'unresolved_unless_iso_or_serial')


def parse_date(book, address, order):
    c = book.cells.get(address)
    require(c is not None and c['formula'] is None, 'journal_date_required')
    try:
        if c['type'] == 'n':
            require(c['value'].isdigit(), 'integer_excel_date_required')
            serial = int(c['value'])
            require(1 <= serial <= 100_000 and not (serial == 60 and not book.date1904), 'unsupported_excel_date')
            origin = dt.date(1904, 1, 1) if book.date1904 else dt.date(1899, 12, 31) if serial < 60 else dt.date(1899, 12, 30)
            return (origin + dt.timedelta(days=serial)).isoformat()
        value = book.text(address)
        if re.fullmatch(r'\d{4}-\d{2}-\d{2}', value):
            return dt.date.fromisoformat(value).isoformat()
        require(order is not None, 'ambiguous_journal_date_order')
        return dt.datetime.strptime(value, '%d/%m/%Y' if order == 'dmy' else '%m/%d/%Y').date().isoformat()
    except (ValueError, OverflowError) as exc:
        if isinstance(exc, AssessmentError):
            raise
        raise AssessmentError('invalid_journal_date') from exc


def journal(book, choice):
    expected = {'B': 'Date', 'C': 'Transaction Type', 'D': '#', 'E': 'Name', 'F': 'Memo/Description', 'G': 'Account', 'H': 'Debit', 'I': 'Credit'}
    require(all(book.text(f'{c}5') == text for c, text in expected.items()), 'journal_header_mismatch')
    order, basis = date_order(book, choice)
    active, groups, accounts, total_row, footer_count = None, [], {}, None, 0
    grand = (0, 0)
    for row in sorted(r for r in book.rows if r > 5):
        cols = nonempty_columns(book, row)
        if not cols:
            # QBO exports can put styled blank spacer rows inside a group.
            # They do not close it; a later numeric subtotal remains required.
            continue
        if total_row is not None:
            require(cols == {'A'}, 'financial_rows_after_journal_total')
            footer_count += 1
            continue
        require(cols <= set('ABCDEFGHI'), 'unexpected_journal_columns')
        is_total = 'A' in cols and book.text(f'A{row}').upper() == 'TOTAL'
        if is_total:
            require(active is None, 'source_control_missing_amount_or_subtotal', {'group_start_row': active['source_start_row'], 'boundary_row': row} if active else None)
            require(cols == {'A', 'H', 'I'}, 'invalid_journal_total_row')
            total_row = row
            grand = (book.amount(f'H{row}'), book.amount(f'I{row}'))
            continue
        require('A' not in cols, 'unexpected_journal_row_label')
        if 'B' in cols:
            require(active is None, 'source_control_missing_amount_or_subtotal', {'group_start_row': active['source_start_row'], 'boundary_row': row} if active else None)
            require('C' in cols, 'journal_transaction_type_required', {'row': row})
            active = {'source_start_row': row, 'date': parse_date(book, f'B{row}', order), 'line_count': 0, 'debit': 0, 'credit': 0}
            if 'G' not in cols and not cols & {'H', 'I'}:
                # A dated header may precede its first actual account line.
                # No monetary value is assigned to the header.
                continue
        if 'G' in cols:
            require(active is not None, 'orphan_journal_account_line')
            require('B' in cols or not cols & set('CD'), 'unsupported_journal_continuation_metadata')
            label = book.text(f'G{row}')
            require(label != '', 'journal_account_label_required', {'row': row})
            require(cols & {'H', 'I'}, 'source_control_missing_amount_or_subtotal', {'row': row, 'group_start_row': active['source_start_row']})
            debit, credit = book.amount(f'H{row}'), book.amount(f'I{row}')
            require(not (debit and credit), 'journal_line_has_both_sides')
            active['line_count'] += 1
            active['debit'] += debit
            active['credit'] += credit
            old = accounts.setdefault(label, {'debit': 0, 'credit': 0, 'line_count': 0})
            old['debit'] += debit
            old['credit'] += credit
            old['line_count'] += 1
        else:
            require(active is not None and cols and cols <= {'H', 'I'}, 'unsupported_journal_subtotal')
            subtotal = (book.amount(f'H{row}'), book.amount(f'I{row}'))
            require(active['line_count'] >= 2 and subtotal == (active['debit'], active['credit']), 'journal_group_subtotal_mismatch')
            require(active['debit'] == active['credit'], 'unbalanced_journal_group')
            groups.append({'source_start_row': active['source_start_row'], 'source_subtotal_row': row, 'date': active['date'], 'line_count': active['line_count'], 'debit_cents': str(active['debit']), 'credit_cents': str(active['credit'])})
            active = None
    require(active is None, 'source_control_missing_amount_or_subtotal', {'group_start_row': active['source_start_row'], 'boundary': 'end_of_sheet'} if active else None)
    require(groups and total_row is not None, 'journal_groups_and_total_required')
    total = (sum(int(g['debit_cents']) for g in groups), sum(int(g['credit_cents']) for g in groups))
    require(total == grand, 'journal_grand_total_mismatch')
    dates = [g['date'] for g in groups]
    return {'status': 'supported_controls_passed', 'group_count': len(groups), 'line_count': sum(g['line_count'] for g in groups), 'date_start': min(dates), 'date_end': max(dates), 'date_order': order, 'date_order_basis': basis, 'total_row': total_row, 'debit_cents': str(total[0]), 'credit_cents': str(total[1]), 'groups': groups, 'accounts': [{'label': label, 'label_ref': sha(label.encode('utf-8')), 'line_count': v['line_count'], 'debit_cents': str(v['debit']), 'credit_cents': str(v['credit']), 'net_debit_cents': str(v['debit'] - v['credit'])} for label, v in sorted(accounts.items())], 'footer_rows': footer_count, 'durable_source_transaction_ids_available': False, 'identity_basis': 'Source workbook hash and report row locators only; no durable source transaction identity.'}


def classify(name):
    stem = PurePosixPath(name).name
    if not stem.lower().endswith('.xlsx'):
        return None
    key = re.sub(r'[^a-z]', '', stem[:-5].lower())
    return KINDS.get(key)


def assess_export_bytes(data, order='auto'):
    report = {'format': 'nxt-qbo-financial-assessment/v1', 'inspector_script_sha256': INSPECTOR_SCRIPT_SHA256, 'inspector_hash_basis': 'raw_local_script_bytes_at_module_load', 'source_sha256': sha(data), 'source_bytes': len(data), 'selection': sorted(TITLES), 'reports': [], 'blockers': [], 'financial_controls_passed': False, 'live_cutover_allowed': False, 'source_completeness_verified': False, 'assumed_currency': 'CAD', 'source_currency_verified': False, 'durable_source_transaction_ids_available': False, 'source_executed': False, 'production_writes': 0, 'network_requests': 0,
              'limits': ['Selected report assessment only. Customer, employee and supplier list contents are never read.', 'Exact label hashes and row locators are not durable source account/transaction IDs or a source-specific import contract.', 'Only decimal literals, local cell references, same-column SUM ranges, parentheses and plus/minus arithmetic are interpreted by a bounded parser. General Excel/external formulas are never executed; numeric cached formula results are never authoritative.', 'Trial balance and journal controls do not establish source completeness, record authenticity, currency, attachments, open items, statutory archive, tax coverage, tenant authorization or independent approval.', 'General ledger, balance sheet and profit/loss are pinned and structurally inventoried only; their full accounting semantics are not assessed.', 'No target import, data migration, cutover approval or source retirement is performed.']}
    books = {}
    try:
        with archive(data) as outer:
            selected = {}
            for item in outer.infolist():
                kind = classify(item.filename)
                if kind:
                    require(kind not in selected, 'duplicate_financial_report_kind')
                    selected[kind] = item.filename
            for required in ('trial_balance', 'journal'):
                require(required in selected, 'trial_balance_and_journal_required')
            for kind in sorted(selected):
                record = None
                try:
                    book = Workbook(member(outer, selected[kind]), kind)
                    books[kind] = book
                    record = book.metadata()
                    if kind == 'trial_balance':
                        record['assessment'] = trial_balance(book)
                        record['semantics'] = 'restricted_trial_balance_controls'
                    elif kind == 'journal':
                        record['structure_findings'] = {
                            'account_rows_without_amount_cells': [r for r in sorted(book.rows) if r > 5 and 'G' in nonempty_columns(book, r) and not nonempty_columns(book, r) & {'H', 'I'}],
                            'dated_header_only_rows': [r for r in sorted(book.rows) if r > 5 and 'B' in nonempty_columns(book, r) and not nonempty_columns(book, r) & {'G', 'H', 'I'}],
                            'blank_spacer_row_count': sum(not nonempty_columns(book, r) for r in book.rows if r > 5),
                        }
                        record['assessment'] = journal(book, order)
                        record['semantics'] = 'restricted_journal_controls'
                    report['reports'].append(record)
                except AssessmentError as exc:
                    blocker = {'report': kind, 'code': str(exc)}
                    if exc.location is not None:
                        blocker['location'] = exc.location
                    report['blockers'].append(blocker)
                    if record is not None:
                        record['semantics'] = 'incomplete_assessment'
                        record['assessment'] = {'status': 'blocked', **blocker}
                        report['reports'].append(record)
        if 'trial_balance' in books:
            require(all(b.company_hash == books['trial_balance'].company_hash for b in books.values()), 'selected_report_company_heading_mismatch')
        if 'trial_balance' in books and 'journal' in books:
            a, b = books['trial_balance'], books['journal']
            require(a.company_hash == b.company_hash, 'financial_company_heading_mismatch')
            require(a.period_label == b.period_label, 'financial_period_heading_mismatch')
            require(a.period_label.strip().lower() == 'all dates', 'only_all_dates_cross_report_profile_supported')
            parsed = {r['report']: r for r in report['reports']}
            if not report['blockers']:
                tb = {a['label']: int(a['net_debit_cents']) for a in parsed['trial_balance']['assessment']['accounts']}
                gl = {a['label']: int(a['net_debit_cents']) for a in parsed['journal']['assessment']['accounts']}
                diffs = [{'label': label, 'label_ref': sha(label.encode('utf-8')), 'tb_net_debit_cents': str(tb.get(label, 0)), 'journal_net_debit_cents': str(gl.get(label, 0)), 'delta_cents': str(tb.get(label, 0) - gl.get(label, 0))} for label in sorted(tb.keys() | gl.keys()) if tb.get(label, 0) != gl.get(label, 0)]
                report['account_net_reconciliation'] = {'basis': 'Exact report labels and all-dates net debit balances, not durable IDs.', 'mismatch_count': len(diffs), 'mismatches': diffs, 'zero_net_journal_labels_absent_from_tb': sum(label not in tb and value == 0 for label, value in gl.items())}
                if diffs:
                    report['blockers'].append({'report': 'cross_report', 'code': 'account_net_balance_mismatch'})
        report['financial_controls_passed'] = not report['blockers']
    except AssessmentError as exc:
        report['blockers'].append({'report': 'archive_or_cross_report', 'code': str(exc)})
    report['report_sha256'] = sha(canonical(report))
    return report


def read_source(path):
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and not path.is_symlink() and 0 < before.st_size <= MAX_SOURCE, 'unsafe_source_file')
    with path.open('rb') as source:
        opened = os.fstat(source.fileno())
        require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) == (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns), 'source_changed_before_read')
        data = source.read(MAX_SOURCE + 1)
        after = os.fstat(source.fileno())
    current = path.lstat()
    # Windows path stat and descriptor stat can expose different ctime meanings.
    # Compare ctime within each API, and common identity/size/mtime across APIs.
    common = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns)
    require(len(data) == before.st_size and common(before) == common(opened) == common(after) == common(current)
            and opened.st_ctime_ns == after.st_ctime_ns and before.st_ctime_ns == current.st_ctime_ns
            and stat.S_ISREG(current.st_mode) and not path.is_symlink(), 'source_changed_during_read')
    return data


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path, help='NEW private evidence directory; parent must exist')
    parser.add_argument('--date-order', choices=('auto', 'dmy', 'mdy'), default='auto')
    args = parser.parse_args(argv)
    try:
        report = assess_export_bytes(read_source(args.source), args.date_order)
        # On Windows, retain the operator-selected private parent's ACL. Python
        # 3.13's special 0700 DACL can exclude a sandbox's restricted token.
        args.out.mkdir(mode=0o777 if os.name == 'nt' else 0o700)
        output = args.out / 'assessment.json'
        with output.open('x', encoding='utf-8') as target:
            json.dump(report, target, indent=2, ensure_ascii=False)
            target.write('\n')
        print(f"Selected financial controls={'PASS' if report['financial_controls_passed'] else 'BLOCKED'}; reports={len(report['reports'])}; blockers={len(report['blockers'])}; live_cutover_allowed=false")
        return 0 if report['financial_controls_passed'] else 2
    except (AssessmentError, OSError, zipfile.BadZipFile) as exc:
        print(f"Local financial assessment failed ({str(exc) if isinstance(exc, AssessmentError) else type(exc).__name__}); no import performed.", file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
