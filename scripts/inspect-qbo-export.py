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
MAX_FORMULA_LENGTH = 4096
# A cached (stored) QuickBooks subtotal or total may differ from the exact sum
# of its lines by binary floating-point noise. Accept at most one cent, with a
# located warning. Individual stored line amounts must still be exact.
CONTROL_TOLERANCE_CENTS = 1
NS = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
REL = '{http://schemas.openxmlformats.org/package/2006/relationships}'
RID = '{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id'
KINDS = {'trialbalance': 'trial_balance', 'journal': 'journal', 'generalledger': 'general_ledger', 'balancesheet': 'balance_sheet', 'profitandloss': 'profit_and_loss'}
TITLES = {'trial_balance': 'trial balance', 'journal': 'journal', 'general_ledger': 'general ledger', 'balance_sheet': 'balance sheet', 'profit_and_loss': 'profit and loss'}
LEGACY_JOURNAL_HEADER = {'B': 'Date', 'C': 'Transaction Type', 'D': '#', 'E': 'Name', 'F': 'Memo/Description', 'G': 'Account', 'H': 'Debit', 'I': 'Credit'}
# QuickBooks Online's current Journal export. Column J ('Adj') flags adjusting entries.
MODERN_JOURNAL_HEADER = {'B': 'Transaction date', 'C': 'Transaction type', 'D': '#', 'E': 'Name', 'F': 'Description', 'G': 'Account Name', 'H': 'Debit', 'I': 'Credit', 'J': 'Adj'}
ADJ_FLAGS = {'yes': True, 'no': False}
# Member names that look like list exports are never opened, not even to read a title.
NONFINANCIAL_NAME = re.compile(r'customer|employee|supplier|vendor|contact|payroll', re.I)
MONTHS = {m: i for i, m in enumerate(('january', 'february', 'march', 'april', 'may', 'june', 'july', 'august', 'september', 'october', 'november', 'december'), 1)}
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
    require(isinstance(formula, str) and 0 < len(formula) <= MAX_FORMULA_LENGTH, 'formula_length_limit')
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


def formula_shape(formula):
    """Describe a formula's structure without any cell values or client text."""
    text = formula if isinstance(formula, str) else ''
    refs = re.findall(r'(?<![A-Z])([A-Z]{1,3})([1-9][0-9]{0,5})(?![0-9(])', text, re.I)
    ranges = re.findall(r'SUM\(', text, re.I)
    literals = re.findall(r'(?<![A-Z0-9.])[0-9]+(?:\.[0-9]+)?(?![0-9A-Z])', re.sub(r'[A-Z]{1,3}[1-9][0-9]{0,5}', '', text, flags=re.I), re.I)
    other = sorted(set(re.sub(r'[A-Z]{1,3}[1-9][0-9]{0,5}|SUM|[0-9.]|\s', '', text, flags=re.I)))
    rows = [int(r) for _, r in refs]
    return {'formula_length': len(text), 'formula_length_limit': MAX_FORMULA_LENGTH, 'cell_reference_count': len(refs), 'referenced_columns': sorted({c.upper() for c, _ in refs}),
            'referenced_row_min': min(rows) if rows else None, 'referenced_row_max': max(rows) if rows else None, 'sum_range_count': len(ranges), 'numeric_literal_count': len(literals), 'operator_characters': other}


class Workbook:
    def __init__(self, data, kind):
        """`kind=None` derives the report kind from the title row (A2)."""
        self.kind, self.raw_hash, self.raw_bytes = kind, sha(data), len(data)
        self.cells, self.rows, self.formula_cache_mismatches, self.missing_formula_caches = {}, {}, [], []
        self.formula_count, self.literal_formula_count, self.sum_formula_count = 0, 0, 0
        self.arithmetic_formula_count, self._eval_work = 0, 0
        # Located, value-free notes: tolerated cached totals and ignored
        # over-length grand-total formulas. Never amounts or client text.
        self.warnings, self.overlength = [], {}
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
            self.sheet_name = sheets[0].get('name', '') if len(sheets) == 1 else ''
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
                    try:
                        if f is not None and isinstance(formula, str) and len(formula) > MAX_FORMULA_LENGTH:
                            # Deferred: allowed only as a journal grand-total cell
                            # (checked below), where it is ignored, never evaluated.
                            compiled = None
                            self.overlength[address] = {'sheet': self.sheet_name, 'cell': address, **formula_shape(formula)}
                        else:
                            compiled = None if f is None else compile_formula(formula)
                    except AssessmentError as exc:
                        raise AssessmentError(str(exc), {'sheet': self.sheet_name, 'cell': address, **formula_shape(formula)}) from exc
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
        if kind is None:
            kind = next((k for k, title in TITLES.items() if self.text('A2').strip().lower() == title), None)
            require(kind is not None, 'report_title_unrecognised')
            self.kind = kind
        require(self.text('A2').strip().lower() == TITLES[kind], 'report_title_mismatch')
        for address, location in sorted(self.overlength.items()):
            # The 4,096-character limit holds for every formula except the
            # journal's cached grand total (row labelled TOTAL, H or I), which
            # is recomputed from the individual lines instead.
            row = int(ADDRESS.fullmatch(address)[2])
            label = self.cells.get(f'A{row}')
            grand_total = kind == 'journal' and address[0] in 'HI' and len(address) > 1 and address[1].isdigit() and label is not None and label['formula'] is None and label['value'].strip().upper() == 'TOTAL'
            require(grand_total, 'formula_length_limit', location)
        self.company_hash = sha(self.text('A1').encode('utf-8'))
        self.period_label = self.text('A3')
        require(self.period_label != '', 'report_period_required')

    def text(self, address):
        c = self.cells.get(address)
        if not c:
            return ''
        require(c['formula'] is None and c['type'] in ('s', 'inlineStr', 'str'), 'text_cell_required')
        return c['value'].strip()

    def _located_numeric(self, address, value, role):
        # Report where a number failed, never the number itself.
        try:
            return numeric(value)
        except AssessmentError as exc:
            if exc.location is not None:
                raise
            decimals = len(value.split('.', 1)[1].split('e')[0].split('E')[0]) if isinstance(value, str) and '.' in value else 0
            raise AssessmentError(str(exc), {'sheet': self.sheet_name, 'cell': address, 'role': role, 'decimal_places': decimals}) from exc

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
        overlength = getattr(self, 'overlength', {})
        require(address not in overlength, 'formula_length_limit', overlength.get(address))
        require(address not in self._evaluating, 'circular_formula')
        require(len(self._evaluating) < MAX_DEPENDENCY_DEPTH, 'formula_dependency_depth_limit')
        self._evaluating.add(address)
        try:
            formula = c['formula']
            if formula is None:
                result = self._located_numeric(address, c['value'], 'stored_value')
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
                else:
                    delta = self.cached_delta(address, c['value'], result)
                    if delta != 0:
                        self.formula_cache_mismatches.append(address)
                        if delta <= CONTROL_TOLERANCE_CENTS:
                            self.warn('cached_total_within_one_cent', address)
            self._evaluated[address] = result
            return result
        finally:
            self._evaluating.remove(address)

    def warn(self, code, address, **extra):
        item = {'code': code, 'sheet': self.sheet_name, 'cell': address, **extra}
        if item not in self.warnings:
            self.warnings.append(item)

    def cached_delta(self, address, value, exact_cents):
        """Absolute cents between a cached/stored control value and the exact sum.

        Returns 0 for an exact match. The value itself is never reported."""
        try:
            return 0 if self._located_numeric(address, value, 'formula_cached_value') == exact_cents else abs(Decimal(value) * 100 - exact_cents)
        except AssessmentError as exc:
            require(str(exc) == 'fractional_cent', str(exc), exc.location)
            with localcontext() as ctx:
                ctx.prec = 80
                return abs(Decimal(value) * 100 - exact_cents)

    def control(self, address, exact_cents):
        """A subtotal/total cell checked against the exact sum of its lines.

        Formula cells are evaluated exactly (their cache is only a cross-check).
        A stored value within one cent is accepted with a located warning; the
        exact line sum is what is returned and used."""
        c = self.cells.get(address)
        if c and c['formula'] is None and c['type'] == 'n' and c['value'] != '':
            delta = self.cached_delta(address, c['value'], exact_cents)
            if delta == 0:
                return exact_cents
            if delta <= CONTROL_TOLERANCE_CENTS:
                self.warn('stored_total_within_one_cent', address)
                return exact_cents
            return None
        return self.amount(address)

    def metadata(self):
        populated = sum(any(self.cells[a]['value'] != '' or self.cells[a]['formula'] is not None for a in cols.values()) for cols in self.rows.values())
        return {'report': self.kind, 'warnings': self.warnings, 'bytes': self.raw_bytes, 'sha256': self.raw_hash, 'populated_rows': populated, 'cell_slots': len(self.cells), 'formula_count': self.formula_count, 'company_heading_sha256': self.company_hash, 'period_label': self.period_label, 'semantics': 'metadata_only'}


def nonempty_columns(book, row):
    return {col for col, address in book.rows.get(row, {}).items() if book.cells[address]['value'] != '' or book.cells[address]['formula'] is not None}


def optional_text(book, address):
    """Header metadata only. Never evaluates a formula or invents a blank amount."""
    cell = book.cells.get(address)
    if not cell or (cell['value'] == '' and cell['formula'] is None):
        return ''
    if cell['formula'] is not None:
        return ''
    return cell['value'].strip() if cell['type'] in ('s', 'inlineStr', 'str') else cell['value']


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
        debit, credit = (None, None) if label.upper() == 'TOTAL' else (book.amount(f'B{row}'), book.amount(f'C{row}'))
        if label.upper() == 'TOTAL':
            require('B' in cols and 'C' in cols, 'tb_total_amounts_required')
            total_row = row
            continue
        require(label and bool(cols & {'B', 'C'}), 'tb_account_amount_required')
        require(label not in seen, 'duplicate_tb_account_label')
        require(not (debit and credit), 'tb_account_has_both_sides')
        seen.add(label)
        accounts.append({'label': label, 'label_ref': sha(label.encode('utf-8')), 'row': row, 'debit_cents': str(debit), 'credit_cents': str(credit), 'net_debit_cents': str(debit - credit)})
    require(accounts and total_row is not None, 'tb_accounts_and_total_required')
    totals = (sum(int(a['debit_cents']) for a in accounts), sum(int(a['credit_cents']) for a in accounts))
    reported = (book.control(f'B{total_row}', totals[0]), book.control(f'C{total_row}', totals[1]))
    require(totals == reported, 'tb_total_mismatch')
    require(totals[0] == totals[1], 'unbalanced_trial_balance')
    return {'status': 'supported_controls_passed', 'account_count': len(accounts), 'accounts': accounts, 'total_row': total_row, 'debit_cents': str(totals[0]), 'credit_cents': str(totals[1]), 'footer_rows': footer_count, 'formula_cache_mismatch_count': len(book.formula_cache_mismatches), 'formula_cache_mismatch_cells': book.formula_cache_mismatches, 'missing_formula_cache_count': len(book.missing_formula_caches), 'literal_formulas_evaluated': book.literal_formula_count, 'sum_formulas_evaluated': book.sum_formula_count, 'arithmetic_formulas_evaluated': book.arithmetic_formula_count, 'warnings': book.warnings, 'identity_basis': 'Exact account label hash and source row, not durable source account IDs.'}


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


def _raw_cells(book, row, cols):
    raw = []
    for col in sorted(cols):
        cell = book.cells.get(f'{col}{row}')
        if cell:
            raw.append([col, True, cell['type'], cell['value'], cell['formula']])
    return raw


def journal_postings(book, choice):
    """Same journal controls as `journal`, plus the valued lines a stager must keep.

    The public assessment omits line payloads. Names, memos and supplied numbers
    are retained here as original cell text. They are not durable source IDs.
    """
    layout = journal_layout(book)
    if layout == 'qbo_modern':
        return modern_journal_postings(book, choice)
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
            continue
        require('A' not in cols, 'unexpected_journal_row_label')
        if 'B' in cols:
            require(active is None, 'source_control_missing_amount_or_subtotal', {'group_start_row': active['source_start_row'], 'boundary_row': row} if active else None)
            require('C' in cols, 'journal_transaction_type_required', {'row': row})
            active = {'source_start_row': row, 'date': parse_date(book, f'B{row}', order), 'line_count': 0, 'debit': 0, 'credit': 0, 'lines': [],
                      'transaction_type': book.text(f'C{row}'), 'number': optional_text(book, f'D{row}'), 'name': optional_text(book, f'E{row}'), 'memo': optional_text(book, f'F{row}')}
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
            active['lines'].append({'row': row, 'label': label, 'debit': debit, 'credit': credit, 'raw_cells': _raw_cells(book, row, cols)})
            old = accounts.setdefault(label, {'debit': 0, 'credit': 0, 'line_count': 0})
            old['debit'] += debit
            old['credit'] += credit
            old['line_count'] += 1
        else:
            require(active is not None and cols and cols <= {'H', 'I'}, 'unsupported_journal_subtotal')
            subtotal = (book.control(f'H{row}', active['debit']), book.control(f'I{row}', active['credit']))
            require(active['line_count'] >= 2 and subtotal == (active['debit'], active['credit']), 'journal_group_subtotal_mismatch')
            require(active['debit'] == active['credit'], 'unbalanced_journal_group')
            groups.append({'source_start_row': active['source_start_row'], 'source_subtotal_row': row, 'date': active['date'], 'line_count': active['line_count'], 'debit_cents': str(active['debit']), 'credit_cents': str(active['credit']),
                           'transaction_type': active['transaction_type'], 'number': active['number'], 'name': active['name'], 'memo': active['memo'], 'lines': active['lines']})
            active = None
    require(active is None, 'source_control_missing_amount_or_subtotal', {'group_start_row': active['source_start_row'], 'boundary': 'end_of_sheet'} if active else None)
    require(groups and total_row is not None, 'journal_groups_and_total_required')
    total = (sum(int(g['debit_cents']) for g in groups), sum(int(g['credit_cents']) for g in groups))
    require(total == grand_total(book, total_row, total), 'journal_grand_total_mismatch')
    dates = [g['date'] for g in groups]
    return {'status': 'supported_controls_passed', 'group_count': len(groups), 'line_count': sum(g['line_count'] for g in groups), 'date_start': min(dates), 'date_end': max(dates), 'date_order': order, 'date_order_basis': basis, 'total_row': total_row, 'debit_cents': str(total[0]), 'credit_cents': str(total[1]), 'groups': groups, 'accounts': [{'label': label, 'label_ref': sha(label.encode('utf-8')), 'line_count': v['line_count'], 'debit_cents': str(v['debit']), 'credit_cents': str(v['credit']), 'net_debit_cents': str(v['debit'] - v['credit'])} for label, v in sorted(accounts.items())], 'footer_rows': footer_count, 'warnings': book.warnings, 'held_lines': [], 'held_group_count': 0, 'durable_source_transaction_ids_available': False, 'identity_basis': 'Source workbook hash and report row locators only; no durable source transaction identity.', 'layout': 'qbo_legacy'}


def grand_total(book, row, exact):
    """The journal TOTAL row, checked against the exact sum of every group.

    An over-length cached grand-total formula is ignored (never evaluated) and
    the total is the computed sum of the individual lines, with a warning."""
    result = []
    for col, value in zip('HI', exact):
        address = f'{col}{row}'
        if address in book.overlength:
            book.warn('overlength_total_formula_ignored', address, formula_length=book.overlength[address]['formula_length'])
            result.append(value)
        else:
            result.append(book.control(address, value))
    return tuple(result)


def journal_layout(book):
    """Exactly one known header layout on row 5; anything else is blocked."""
    header = {c: book.text(a) for c, a in book.rows.get(5, {}).items() if book.text(a) != ''}
    if header == LEGACY_JOURNAL_HEADER:
        return 'qbo_legacy'
    if header == MODERN_JOURNAL_HEADER:
        return 'qbo_modern'
    raise AssessmentError('journal_header_mismatch')


def modern_journal_postings(book, choice):
    """QBO's current Journal layout, under the same controls as the legacy one.

    Groups open with a column-A transaction label and close with
    'Total for <label>' (H/I subtotal). Every line repeats date and type in B/C,
    carries its account in G, exactly one valued side in H/I and the adjusting
    flag in J. Blank amounts are never imputed as zero; a group's lines must
    share date, type and adjusting flag; group and grand totals must reconcile.
    The column-A label is source report text, not a durable transaction id.
    """
    order, basis = date_order(book, choice)
    active, groups, accounts, total_row, footer_count, held, held_groups = None, [], {}, None, 0, [], []
    for row in sorted(r for r in book.rows if r > 5):
        cols = nonempty_columns(book, row)
        if not cols:
            continue
        if total_row is not None:
            require(cols == {'A'}, 'financial_rows_after_journal_total')
            footer_count += 1
            continue
        require(cols <= set('ABCDEFGHIJ'), 'unexpected_journal_columns')
        if 'A' in cols:
            label = book.text(f'A{row}')
            missing = {'group_start_row': active['source_start_row'], 'boundary_row': row} if active else None
            if label.upper() == 'TOTAL':
                require(active is None, 'source_control_missing_amount_or_subtotal', missing)
                require(cols == {'A', 'H', 'I'}, 'invalid_journal_total_row')
                total_row = row
                continue
            if label.startswith('Total for '):
                require(active is not None and label[len('Total for '):] == active['group_label'], 'journal_group_total_label_mismatch', {'row': row})
                require(cols == {'A', 'H', 'I'}, 'source_control_missing_amount_or_subtotal', {'row': row, 'group_start_row': active['source_start_row']})
                subtotal = (book.control(f'H{row}', active['debit']), book.control(f'I{row}', active['credit']))
                require(active['line_count'] >= 2 and subtotal == (active['debit'], active['credit']), 'journal_group_subtotal_mismatch', {'row': row})
                require(active['debit'] == active['credit'], 'unbalanced_journal_group', {'row': row})
                group = {'source_start_row': active['source_start_row'], 'source_subtotal_row': row, 'date': active['date'], 'line_count': active['line_count'], 'debit_cents': str(active['debit']), 'credit_cents': str(active['credit']),
                         'transaction_type': active['transaction_type'], 'number': active['number'], 'name': active['name'], 'memo': active['memo'], 'lines': active['lines'],
                         'adjusting': active['adjusting'], 'source_group_label': active['group_label']}
                if active['held_lines']:
                    # The whole entry is held for review: none of its lines post.
                    held.extend(active['held_lines'])
                    held_groups.append({**group, 'held_lines': active['held_lines']})
                else:
                    groups.append(group)
                    for line in active['lines']:
                        old = accounts.setdefault(line['label'], {'debit': 0, 'credit': 0, 'line_count': 0})
                        old['debit'] += line['debit']
                        old['credit'] += line['credit']
                        old['line_count'] += 1
                active = None
                continue
            require(cols == {'A'}, 'unexpected_journal_row_label', {'row': row})
            require(active is None, 'source_control_missing_amount_or_subtotal', missing)
            active = {'source_start_row': row, 'group_label': label, 'date': None, 'line_count': 0, 'debit': 0, 'credit': 0, 'lines': [],
                      'transaction_type': None, 'number': '', 'name': '', 'memo': '', 'adjusting': None, 'held_lines': []}
            continue
        require(active is not None, 'orphan_journal_account_line', {'row': row})
        require('B' in cols and 'C' in cols, 'journal_transaction_type_required', {'row': row})
        require('J' in cols and book.text(f'J{row}').lower() in ADJ_FLAGS, 'journal_adjustment_flag_required', {'row': row})
        date, ttype, adjusting = parse_date(book, f'B{row}', order), book.text(f'C{row}'), ADJ_FLAGS[book.text(f'J{row}').lower()]
        if active['date'] is None:
            active.update(date=date, transaction_type=ttype, adjusting=adjusting, number=optional_text(book, f'D{row}'), name=optional_text(book, f'E{row}'))
        require((date, ttype) == (active['date'], active['transaction_type']), 'journal_group_metadata_inconsistent', {'row': row})
        require(adjusting == active['adjusting'], 'journal_adjustment_flag_inconsistent', {'row': row})
        if not active['memo']:
            active['memo'] = optional_text(book, f'F{row}')
        label = book.text(f'G{row}') if 'G' in cols else ''
        valued = bool(cols & {'H', 'I'})
        debit, credit = (book.amount(f'H{row}'), book.amount(f'I{row}')) if valued else (0, 0)
        require(not (debit and credit), 'journal_line_has_both_sides')
        active['line_count'] += 1
        # Present stored amounts still count toward the group's subtotal
        # control. A blank amount contributes nothing and is never imputed.
        active['debit'] += debit
        active['credit'] += credit
        if not label or not valued:
            # Never fill in or guess an account or an amount: hold the line
            # (and so its whole entry) for human review. No suspense posting.
            reason = 'amount_without_account' if valued else 'account_without_amount' if label else 'no_account_no_amount'
            active['held_lines'].append({'row': row, 'entry_start_row': active['source_start_row'], 'reason': reason,
                                         'debit_cents': str(debit) if valued else None, 'credit_cents': str(credit) if valued else None,
                                         'account_label_ref': sha(label.encode('utf-8')) if label else None, 'raw_cells': _raw_cells(book, row, cols)})
            continue
        active['lines'].append({'row': row, 'label': label, 'debit': debit, 'credit': credit, 'adjusting': adjusting, 'memo': optional_text(book, f'F{row}'), 'name': optional_text(book, f'E{row}'), 'raw_cells': _raw_cells(book, row, cols)})
    require(active is None, 'source_control_missing_amount_or_subtotal', {'group_start_row': active['source_start_row'], 'boundary': 'end_of_sheet'} if active else None)
    require((groups or held_groups) and total_row is not None, 'journal_groups_and_total_required')
    every = groups + held_groups
    total = (sum(int(g['debit_cents']) for g in every), sum(int(g['credit_cents']) for g in every))
    require(total == grand_total(book, total_row, total), 'journal_grand_total_mismatch')
    dates = [g['date'] for g in every]
    return {'status': 'supported_controls_passed', 'group_count': len(groups), 'line_count': sum(g['line_count'] for g in groups),
            'held_lines': held, 'held_group_count': len(held_groups), 'held_groups': held_groups, 'warnings': book.warnings, 'date_start': min(dates), 'date_end': max(dates), 'date_order': order, 'date_order_basis': basis, 'total_row': total_row, 'debit_cents': str(total[0]), 'credit_cents': str(total[1]), 'groups': groups, 'accounts': [{'label': label, 'label_ref': sha(label.encode('utf-8')), 'line_count': v['line_count'], 'debit_cents': str(v['debit']), 'credit_cents': str(v['credit']), 'net_debit_cents': str(v['debit'] - v['credit'])} for label, v in sorted(accounts.items())], 'footer_rows': footer_count, 'adjusting_group_count': sum(g['adjusting'] for g in groups), 'durable_source_transaction_ids_available': False, 'identity_basis': 'Source workbook hash, report row locators and column-A group text only; no durable source transaction identity.', 'layout': 'qbo_modern'}


def journal(book, choice):
    detail = journal_postings(book, choice)
    detail['groups'] = [{k: g[k] for k in ('source_start_row', 'source_subtotal_row', 'date', 'line_count', 'debit_cents', 'credit_cents', 'adjusting') if k in g} for g in detail['groups']]
    detail.pop('held_groups', None)
    detail['held_lines'] = [{k: v for k, v in h.items() if k != 'raw_cells'} for h in detail['held_lines']]
    return detail


def selected_workbooks(data):
    """Re-open the trial balance and journal after a separate assessment pass."""
    books, names = {}, {}
    with archive(data) as outer:
        selected = select_reports(outer)
        for required in ('trial_balance', 'journal'):
            require(required in selected, 'trial_balance_and_journal_required')
        for kind in ('trial_balance', 'journal'):
            books[kind] = Workbook(member(outer, selected[kind]), kind)
            names[kind] = selected[kind]
    return books, names


def classify(name):
    """Exact canonical filename -> kind (legacy behaviour)."""
    stem = PurePosixPath(name).name
    if not stem.lower().endswith('.xlsx'):
        return None
    key = re.sub(r'[^a-z]', '', stem[:-5].lower())
    return KINDS.get(key)


def name_hint(name):
    """Kind suggested by a non-canonical filename such as `Co_Journal_2017.xlsx`."""
    stem = PurePosixPath(name).name
    if not stem.lower().endswith('.xlsx'):
        return None
    key = re.sub(r'[^a-z]', '', stem[:-5].lower())
    hits = {kind for token, kind in KINDS.items() if token in key}
    return next(iter(hits)) if len(hits) == 1 else None


def select_reports(outer):
    """Pick financial reports by exact name, or by title row (A2) and content.

    Exact canonical names keep their legacy meaning. Other .xlsx members are
    opened under the same bounded parser and classified by their title row; a
    name hint must agree with the title. Members whose names look like list
    exports (customers, employees, suppliers...) are never opened.
    """
    selected = {}
    for item in outer.infolist():
        name = item.filename
        if name.endswith('/'):
            continue
        kind = classify(name)
        if kind is None:
            if not name.lower().endswith('.xlsx') or NONFINANCIAL_NAME.search(PurePosixPath(name).name):
                continue
            hint = name_hint(name)
            try:
                kind = Workbook(member(outer, name), None).kind
            except AssessmentError:
                if hint is None:
                    continue
                raise
            require(hint is None or hint == kind, 'report_name_title_conflict', {'member': name})
            if kind not in ('trial_balance', 'journal') and hint is None:
                continue
        require(kind not in selected, 'duplicate_financial_report_kind')
        selected[kind] = name
    return selected


def parse_period(label):
    """`January-December, 2017` -> (start, end); `As of December 31, 2017` -> (None, date)."""
    text = re.sub(r'\s+', ' ', label.strip().lower())
    m = re.fullmatch(r'([a-z]+)\s*-\s*([a-z]+),? (\d{4})', text)
    if m and m[1] in MONTHS and m[2] in MONTHS and MONTHS[m[1]] <= MONTHS[m[2]]:
        year, last = int(m[3]), MONTHS[m[2]]
        end = (dt.date(year + (last == 12), last % 12 + 1, 1) - dt.timedelta(days=1))
        return dt.date(year, MONTHS[m[1]], 1), end
    m = re.fullmatch(r'as of ([a-z]+) (\d{1,2}),? (\d{4})', text)
    if m and m[1] in MONTHS:
        try:
            return None, dt.date(int(m[3]), MONTHS[m[1]], int(m[2]))
        except ValueError:
            return None
    return None


def period_profile(tb_label, journal_label):
    """Accept identical headings (legacy all-dates profile) or a journal period
    paired with a trial balance as of exactly that period's end."""
    if tb_label == journal_label:
        require(tb_label.strip().lower() == 'all dates', 'only_all_dates_cross_report_profile_supported')
        return {'kind': 'all_dates'}
    tb, jr = parse_period(tb_label), parse_period(journal_label)
    require(tb is not None and jr is not None and tb[0] is None and jr[0] is not None and tb[1] == jr[1], 'financial_period_heading_mismatch')
    return {'kind': 'journal_period_with_tb_as_of_period_end', 'period_start': jr[0].isoformat(), 'period_end': jr[1].isoformat()}


def prior_trial_balance(data):
    """Closing trial balance of the prior year's export, used as this year's opening.

    Only the trial balance is read. Returns label -> net debit cents, its
    as-of date and its source hash. Amounts are never reported by label."""
    with archive(data) as outer:
        selected = select_reports(outer)
        require('trial_balance' in selected, 'prior_trial_balance_required')
        book = Workbook(member(outer, selected['trial_balance']), 'trial_balance')
    period = parse_period(book.period_label)
    require(period is not None and period[0] is None, 'prior_trial_balance_must_be_as_of')
    tb = trial_balance(book)
    return {'balances': {a['label']: int(a['net_debit_cents']) for a in tb['accounts']}, 'as_of': period[1].isoformat(),
            'sha256': book.raw_hash, 'company_hash': book.company_hash}


def reconcile_opening(tb, gl, opening, before_month=None):
    """Exact per-account check TB = opening + journal activity.

    Without an exact match, accept only QuickBooks' year-end close: every
    differing account resets exactly and exactly one other account (retained
    earnings) absorbs their sum. The close is tested at the calendar boundary
    (fiscal year starting January) and, when `before_month` is given
    ({month: {label: net activity dated before that month}}), at each other
    fiscal-year start month: an income-statement account then closes its
    opening plus its activity before the fiscal-year start. A month is
    accepted only if every differing account fits to the cent; nothing is
    estimated. Mismatches list only the accounts the best close leaves
    unexplained, so a single stray posting no longer reports every reset
    income-statement account as mismatched. Returns the closed opening actually
    carried, or None."""
    labels = sorted(tb.keys() | gl.keys() | opening.keys())
    delta = {l: tb.get(l, 0) - opening.get(l, 0) - gl.get(l, 0) for l in labels}
    diffs = [l for l in labels if delta[l]]
    if not diffs:
        return {'status': 'reconciled_exact', 'closed_opening': dict(opening), 'year_end_close_accounts': 0, 'mismatches': []}
    best = None
    for month in [1] + sorted(m for m in (before_month or {}) if m != 1):
        pre = (before_month or {}).get(month, {})
        closes = {l: opening.get(l, 0) + pre.get(l, 0) for l in labels}
        reset = [l for l in diffs if closes[l] and delta[l] == -closes[l]]
        rest = [l for l in diffs if l not in reset]
        closed_sum = sum(closes[l] for l in reset)
        receivers = [l for l in rest if delta[l] == closed_sum]
        if reset and receivers:
            rest = [l for l in rest if l != receivers[0]]
        else:
            receivers = []
        candidate = (len(rest), month, reset, receivers, rest)
        if best is None or candidate[0] < best[0]:
            best = candidate
    unexplained, month, reset, receivers, rest = best
    if not unexplained and receivers:
        receiver = receivers[0]
        # closed_opening is the opening that, with this period's journal,
        # yields the trial balance. For a January fiscal start it is the
        # prior closing balance after QuickBooks' close; for another start
        # month the close happens inside the period and is carried separately.
        closed = {l: opening.get(l, 0) + (delta[l] if l in reset or l == receiver else 0) for l in labels}
        closed = {l: v for l, v in closed.items() if v or l in opening}
        out = {'status': 'reconciled_with_year_end_close', 'closed_opening': closed, 'year_end_close_accounts': len(reset),
               'retained_earnings_label_ref': sha(receiver.encode('utf-8')), 'fiscal_year_start_month': month, 'mismatches': []}
        if month != 1:
            out['year_end_close'] = {l: delta[l] for l in reset + [receiver]}
        return out
    return {'status': 'unreconciled', 'closed_opening': None, 'year_end_close_accounts': len(reset) if receivers else 0,
            'fiscal_year_start_month_tested': month if receivers else None,
            'mismatches': [{'label_ref': sha(l.encode('utf-8')), 'delta_cents': str(delta[l])} for l in (rest if receivers else diffs)]}


def activity_before_months(lines_by_date, period_start):
    """{month: {label: net debit cents dated before that month}} for months 2-12."""
    start = dt.date.fromisoformat(period_start)
    out = {}
    for month in range(2, 13):
        cut = dt.date(start.year, month, 1).isoformat()
        acc = {}
        for date, label, net in lines_by_date:
            if date < cut:
                acc[label] = acc.get(label, 0) + net
        out[month] = acc
    return out


def assess_export_bytes(data, order='auto', prior=None, first_year=False):
    """Assess one export. `prior` is prior_trial_balance() of the year before
    (multi-year carry-forward). `first_year` marks the first year of a
    multi-year run without a prior trial balance: an opening-balance
    difference is then reported as partial history (a review blocker), not
    resolved by guessing."""
    report = {'format': 'nxt-qbo-financial-assessment/v1', 'inspector_script_sha256': INSPECTOR_SCRIPT_SHA256, 'inspector_hash_basis': 'raw_local_script_bytes_at_module_load', 'source_sha256': sha(data), 'source_bytes': len(data), 'selection': sorted(TITLES), 'reports': [], 'blockers': [], 'financial_controls_passed': False, 'live_cutover_allowed': False, 'source_completeness_verified': False, 'assumed_currency': 'CAD', 'source_currency_verified': False, 'durable_source_transaction_ids_available': False, 'source_executed': False, 'production_writes': 0, 'network_requests': 0,
              'limits': ['Selected report assessment only. Members named like customer, employee, supplier, vendor, contact or payroll lists are never opened; other non-canonical .xlsx members are opened by the same bounded parser only to read their title row.', 'Exact label hashes and row locators are not durable source account/transaction IDs or a source-specific import contract.', 'Only decimal literals, local cell references, same-column SUM ranges, parentheses and plus/minus arithmetic are interpreted by a bounded parser. General Excel/external formulas are never executed; numeric cached formula results are never authoritative.', 'Trial balance and journal controls do not establish source completeness, record authenticity, currency, attachments, open items, statutory archive, tax coverage, tenant authorization or independent approval.', 'General ledger, balance sheet and profit/loss are pinned and structurally inventoried only; their full accounting semantics are not assessed.', 'No target import, data migration, cutover approval or source retirement is performed.']}
    books = {}
    try:
        with archive(data) as outer:
            selected = select_reports(outer)
            report['selected_members'] = {k: v for k, v in sorted(selected.items())}
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
            profile = period_profile(a.period_label, b.period_label)
            report['period_profile'] = profile
            parsed = {r['report']: r for r in report['reports']}
            if profile['kind'] != 'all_dates' and 'journal' in parsed and parsed['journal']['assessment'].get('status') == 'supported_controls_passed':
                j = parsed['journal']['assessment']
                if not (profile['period_start'] <= j['date_start'] and j['date_end'] <= profile['period_end']):
                    report['blockers'].append({'report': 'cross_report', 'code': 'journal_date_outside_period'})
            if not report['blockers'] and (prior is not None or first_year or parsed['journal']['assessment'].get('held_lines')):
                reconcile_year(report, parsed, profile, prior, b, order)
            elif not report['blockers']:
                tb = {a['label']: int(a['net_debit_cents']) for a in parsed['trial_balance']['assessment']['accounts']}
                gl = {a['label']: int(a['net_debit_cents']) for a in parsed['journal']['assessment']['accounts']}
                diffs = [{'label': label, 'label_ref': sha(label.encode('utf-8')), 'tb_net_debit_cents': str(tb.get(label, 0)), 'journal_net_debit_cents': str(gl.get(label, 0)), 'delta_cents': str(tb.get(label, 0) - gl.get(label, 0))} for label in sorted(tb.keys() | gl.keys()) if tb.get(label, 0) != gl.get(label, 0)]
                report['account_net_reconciliation'] = {'basis': 'Exact report labels; trial balance net debit balances versus journal net debit activity for the reported period, not durable IDs.', 'mismatch_count': len(diffs), 'mismatches': diffs, 'zero_net_journal_labels_absent_from_tb': sum(label not in tb and value == 0 for label, value in gl.items())}
                if diffs:
                    report['blockers'].append({'report': 'cross_report', 'code': 'account_net_balance_mismatch'})
        report['financial_controls_passed'] = not report['blockers']
    except AssessmentError as exc:
        report['financial_controls_passed'] = False
        blocker = {'report': 'archive_or_cross_report', 'code': str(exc)}
        if exc.location is not None:
            blocker['location'] = exc.location
        report['blockers'].append(blocker)
    parsed = {r['report']: r.get('assessment', {}) for r in report['reports']}
    j = parsed.get('journal', {})
    report['warnings'] = [{'report': k, **w} for k in ('trial_balance', 'journal') for w in parsed.get(k, {}).get('warnings', [])]
    report['held_for_review'] = [{k: h[k] for k in ('row', 'entry_start_row', 'reason')} for h in j.get('held_lines', [])]
    review = report.setdefault('review_blockers', [])
    if report['held_for_review'] and 'SUSPENSE_REVIEW' not in review:
        # Lines held for review block production and G5 promotion until a
        # human resolves them. No suspense account or posting is created.
        review.insert(0, 'SUSPENSE_REVIEW')
    report['report_sha256'] = sha(canonical(report))
    return report


def reconcile_year(report, parsed, profile, prior, journal_book, order='auto'):
    tb = {a['label']: int(a['net_debit_cents']) for a in parsed['trial_balance']['assessment']['accounts']}
    gl = {a['label']: int(a['net_debit_cents']) for a in parsed['journal']['assessment']['accounts']}
    held = bool(parsed['journal']['assessment'].get('held_lines'))
    review = report.setdefault('review_blockers', [])
    rec = {'basis': 'Exact report labels; trial balance = opening (prior closing trial balance) + journal activity, per account. Labels reported by hash only.'}
    if prior is not None:
        require(profile['kind'] != 'all_dates', 'prior_trial_balance_requires_period_profile')
        require(prior['company_hash'] == journal_book.company_hash, 'prior_trial_balance_company_mismatch')
        require(dt.date.fromisoformat(prior['as_of']) + dt.timedelta(days=1) == dt.date.fromisoformat(profile['period_start']), 'prior_trial_balance_not_contiguous')
        opening, rec['opening_source'] = prior['balances'], {'kind': 'prior_closing_trial_balance', 'sha256': prior['sha256'], 'as_of': prior['as_of']}
    else:
        opening, rec['opening_source'] = {}, {'kind': 'none_first_year_partial_history' if profile['kind'] != 'all_dates' else 'all_dates'}
    before_month = None
    if profile['kind'] != 'all_dates' and profile['period_start'][5:] == '01-01':
        postings = journal_postings(journal_book, order)
        lines = [(g['date'], l['label'], l['debit'] - l['credit']) for g in postings['groups'] for l in g['lines']]
        before_month = activity_before_months(lines, profile['period_start'])
    result = reconcile_opening(tb, gl, opening, before_month)
    rec.update({k: v for k, v in result.items() if k != 'closed_opening'})
    report['_closed_opening'] = result['closed_opening']
    if result['status'] == 'unreconciled':
        if held:
            rec['status'] = 'unreconciled_pending_review'
        elif prior is None and profile['kind'] != 'all_dates':
            rec['status'] = 'partial_history_opening_unknown'
            review.append('OPENING_BALANCE_UNRECONCILED')
        else:
            report['blockers'].append({'report': 'cross_report', 'code': 'account_net_balance_mismatch'})
    report['opening_reconciliation'] = rec


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
