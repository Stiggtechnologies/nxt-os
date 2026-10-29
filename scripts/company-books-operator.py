#!/usr/bin/env python3
"""Explicit source-bound CompanyBooks preview/commit/verify. Standard library only.

Never emits record values or credentials. Readback bundles are private financial
artifacts and must remain outside Git checkouts. At most two extra GET attempts
per run handle transient reset/timeout errors; writes never retry automatically.
After an ambiguous write failure, rerun the write
with the exact command and request ID against its immutable intent. For an
interrupted verify, preserve its partial output and use a NEW readback directory
and verification receipt path with the same immutable commit receipt. Partial
files are not acceptance evidence. `--allow-native-adjustments` recomputes
native deltas from returned entries and does not trust the server's delta
fields. It still cannot authorize live cutover. Original systems remain
authoritative.

The workspace key is the HTTP gate, not a staff session and not a Supabase
service-role key. The API persists company books with the service role. That
credential is intentional. Each company-books key it reads or writes is
appended to the company access audit. Staff SQL sessions are limited by
row-level security to assigned companies.
"""
import argparse
import copy
import datetime as dt
import errno
import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import re
import stat
import ssl
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

MAX_CHUNK = 512 * 1024
MAX_BOOK = 24 * 1024 * 1024
MAX_RESPONSE = 5 * 1024 * 1024
SHA = re.compile(r"[0-9a-f]{64}\Z")
IDENT = re.compile(r"[A-Za-z0-9_.-]{1,96}\Z")


class OperatorError(Exception):
    pass


class SafeParser(argparse.ArgumentParser):
    def error(self, _message):
        print('{"ok":false,"error":"CLI_ARGUMENTS_INVALID_USE_HELP"}', file=sys.stderr)
        raise SystemExit(2)


def need(condition, code):
    if not condition:
        raise OperatorError(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def dump(value, ordered=False):
    return json.dumps(value, ensure_ascii=False, allow_nan=False,
                      separators=(",", ":"), sort_keys=not ordered).encode("utf-8")


def parse(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, "DUPLICATE_JSON_KEY")
            result[key] = value
        return result
    def floating(value):
        number = float(value)
        need(math.isfinite(number), "NONFINITE_JSON")
        return number
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=pairs,
                          parse_float=floating,
                          parse_constant=lambda _: need(False, "NONFINITE_JSON"))
    except (UnicodeError, ValueError, RecursionError):
        raise OperatorError("INVALID_JSON") from None


def ordered(value, names, optional=()):
    need(isinstance(value, dict) and set(value) <= set(names), "SCHEMA_CHANGED")
    need(set(names) - set(optional) <= set(value), "SCHEMA_INCOMPLETE")
    return {name: value[name] for name in names if name in value}


def basis(value):
    if value is None:
        return None
    if value.get("kind") == "source_report_row":
        return ordered(value, ["kind", "report_sha256", "row"])
    if value.get("kind") == "derived_zero_after_year_end_close":
        return ordered(value, ["kind", "journal_sha256", "source_row_count", "journal_debit_cents", "journal_credit_cents",
                               "close_line_count", "close_debit_cents", "close_credit_cents"])
    need(value.get("kind") == "derived_zero_journal_omission", "CONTROL_BASIS_UNKNOWN")
    return ordered(value, ["kind", "journal_sha256", "source_row_count",
                           "journal_debit_cents", "journal_credit_cents"])


def preview_bytes(value):
    """Exact current Rust struct order. Preview schema contains no floating values."""
    p = ordered(value, ["format", "company_id", "input", "analysis"])
    i = ordered(p["input"], ["request_id", "expected_head", "company", "source", "chunks", "trial_balance"])
    i["company"] = ordered(i["company"], ["name", "source_company_id", "currency", "identity_verified", "currency_verified"])
    s = ordered(i["source"], ["system", "archive_sha256", "journal_sha256", "trial_balance_sha256", "period_start", "period_end", "full_history", "record_id_coverage", "period_lock"], ["period_lock"])
    if "period_lock" in s:
        s["period_lock"] = ordered(s["period_lock"], ["verified", "closed_through", "evidence_sha256"])
    i["source"] = s
    i["trial_balance"] = [ordered(c, ["account_id", "debit_cents", "credit_cents", "basis", "held_residual"], ["basis", "held_residual"]) for c in i["trial_balance"]]
    for c in i["trial_balance"]:
        if "held_residual" in c:
            c["held_residual"] = ordered(c["held_residual"], ["debit_cents", "credit_cents"])
    for c in i["trial_balance"]:
        if "basis" in c:
            c["basis"] = basis(c["basis"])
    p["input"] = i
    a = ordered(p["analysis"], ["accounts", "entries", "lines", "total_debit_cents", "total_credit_cents", "balances", "blockers", "classified_reports_ready", "controls_passed", "independent_trial_balance_accounts", "derived_zero_accounts", "held_residual_accounts", "control_provenance_complete", "chart_of_accounts_complete"], ["held_residual_accounts"])
    a["balances"] = [ordered(b, ["account_id", "name", "kind", "debit_cents", "credit_cents", "journal_debit_cents", "journal_credit_cents", "source_control_basis"]) for b in a["balances"]]
    for b in a["balances"]:
        b["source_control_basis"] = basis(b["source_control_basis"])
    p["analysis"] = a
    return dump(p, ordered=True)


def initial_head_bytes(value):
    h = ordered(value, ["format", "company_id", "preview_id", "revision", "committed_at", "commit_request_id", "reviewer_label", "previous_head", "journal_chunks", "locked_through", "operation"], ["locked_through", "operation"])
    need(h["format"] == "nxt-company-book-head/v1" and h["revision"] == 1 and "operation" not in h, "NOT_INITIAL_IMPORT_RECEIPT")
    return dump(h, ordered=True)


def cents(value, aggregate=False):
    need(isinstance(value, str) and re.fullmatch(r"0|[1-9][0-9]*", value) is not None, "INVALID_CENTS")
    n = int(value)
    need(n <= (2 ** 127 - 1 if aggregate else 9_007_199_254_740_991), "CENTS_BOUND")
    return n


def origin(value):
    try:
        p = urllib.parse.urlsplit(value)
        port = p.port
    except ValueError:
        raise OperatorError("DESTINATION_NOT_ALLOWED") from None
    need(not p.username and not p.password and not p.query and not p.fragment and p.path in ("", "/"), "DESTINATION_NOT_ALLOWED")
    local = p.hostname in ("localhost", "127.0.0.1", "::1")
    need((local and p.scheme in ("http", "https") and (port is None or 0 < port < 65536)) or
         (p.scheme == "https" and p.hostname == "os.nxtcorporation.ca" and port is None), "DESTINATION_NOT_ALLOWED")
    return urllib.parse.urlunsplit((p.scheme, p.netloc, "", "", "")), local


class PrivateFiles:
    def __init__(self, root):
        self.root = Path(root).resolve(strict=True)
        need(self.root.is_dir(), "PRIVATE_ROOT_INVALID")

    def path(self, name, exists=True):
        candidate = Path(name)
        if not candidate.is_absolute():
            candidate = self.root / candidate
        candidate = Path(os.path.abspath(candidate))
        need(candidate.is_relative_to(self.root), "PATH_ESCAPES_PRIVATE_ROOT")
        current = candidate
        while current != self.root:
            if current.exists():
                st = current.lstat()
                need(not stat.S_ISLNK(st.st_mode) and not getattr(st, "st_file_attributes", 0) & 1024, "REPARSE_PATH_REJECTED")
            current = current.parent
        candidate = candidate.resolve(strict=False)
        need(candidate.is_relative_to(self.root), "PATH_ESCAPES_PRIVATE_ROOT")
        if exists:
            need(candidate.is_file(), "PRIVATE_FILE_MISSING")
        return candidate

    def read(self, name, maximum=MAX_BOOK, expected=None):
        path = self.path(name)
        before = path.stat()
        need(0 < before.st_size <= maximum, "PRIVATE_FILE_SIZE_BOUND")
        raw = path.read_bytes()
        after = path.stat()
        need((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
             (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) and len(raw) == after.st_size, "PRIVATE_FILE_CHANGED")
        if expected is not None:
            need(SHA.fullmatch(expected) is not None and sha(raw) == expected, "PRIVATE_FILE_HASH_MISMATCH")
        return raw

    def json(self, name, expected=None, maximum=MAX_BOOK):
        return parse(self.read(name, maximum, expected))

    def write(self, name, value, same_ok=False):
        path = self.output_path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        raw = dump(value)
        if path.exists():
            need(same_ok and path.stat().st_size == len(raw), "IMMUTABLE_OUTPUT_EXISTS")
            need(self.read(path, max(len(raw), 1)) == raw, "IMMUTABLE_OUTPUT_EXISTS")
            return sha(raw)
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "wb") as handle:
                handle.write(raw)
                handle.flush()
                os.fsync(handle.fileno())
        except FileExistsError:
            raise OperatorError("IMMUTABLE_OUTPUT_EXISTS") from None
        return sha(raw)

    def output_path(self, name):
        path = self.path(name, exists=False)
        need(path.name != ".git", "PRIVATE_OUTPUT_INSIDE_GIT_CHECKOUT")
        current = path.parent
        while True:
            marker = current / ".git"
            try:
                marker.lstat()
            except FileNotFoundError:
                pass
            except OSError:
                raise OperatorError("PRIVATE_OUTPUT_LOCATION_NOT_VERIFIABLE") from None
            else:
                # Both a normal .git directory and a worktree .git file count.
                raise OperatorError("PRIVATE_OUTPUT_INSIDE_GIT_CHECKOUT")
            if current == current.parent:
                break
            current = current.parent
        return path


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise OperatorError("REDIRECT_REJECTED")


class Transport:
    def __init__(self, address, token):
        self.origin, self.local = origin(address)
        need(isinstance(token, str) and token and token == token.strip() and not any(ord(c) < 32 for c in token), "NXT_COMPANY_BOOKS_KEY_REQUIRED")
        self._token = token
        self._opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
        # One CLI operation owns one transport. At most two extra GET attempts
        # are shared across the whole run, not multiplied by the page count.
        self.get_retries_used = 0

    @staticmethod
    def _transient_get_failure(error):
        reason = error.reason if isinstance(error, urllib.error.URLError) else error
        if isinstance(reason, ssl.SSLError):
            return False
        if isinstance(reason, (TimeoutError, ConnectionResetError, ConnectionAbortedError)):
            return True
        retryable = {errno.ETIMEDOUT, errno.ECONNRESET, errno.ECONNABORTED, 10053, 10054, 10060}
        return isinstance(reason, OSError) and (getattr(reason, "errno", None) in retryable or
                                                getattr(reason, "winerror", None) in retryable)

    def request(self, method, route, body=None):
        need(route.startswith("/api/") and "#" not in route and "\\" not in route, "ROUTE_INVALID")
        payload = None if body is None else dump(body)
        need(payload is None or len(payload) <= 4 * 1024 * 1024, "REQUEST_SIZE_BOUND")
        req = urllib.request.Request(self.origin + route, data=payload, method=method,
                                     headers={"Authorization": "Bearer " + self._token,
                                              "Content-Type": "application/json", "Accept": "application/json"})
        while True:
            try:
                with self._opener.open(req, timeout=90) as response:
                    need(response.status == 200, "HTTP_STATUS_REJECTED")
                    if route.startswith("/api/company-books"):
                        need("no-store" in response.headers.get("Cache-Control", ""), "PRIVATE_CACHE_CONTROL_MISSING")
                    raw = response.read(MAX_RESPONSE + 1)
                    need(len(raw) <= MAX_RESPONSE, "RESPONSE_SIZE_BOUND")
                    return parse(raw)
            except urllib.error.HTTPError as exc:
                raise OperatorError("HTTP_" + str(exc.code)) from None
            except http.client.IncompleteRead:
                # A declared HTTP body was truncated: do not disguise a corrupt
                # response as a successful schema/financial readback.
                raise OperatorError("RESPONSE_INCOMPLETE_NO_RETRY") from None
            except (urllib.error.URLError, TimeoutError, OSError) as exc:
                retryable = method == "GET" and body is None and self._transient_get_failure(exc)
                if retryable and self.get_retries_used < 2:
                    self.get_retries_used += 1
                    time.sleep(0.1 * self.get_retries_used)
                    # The exact origin/route/method/body/auth headers and TLS
                    # policy are retained; POST is never automatically replayed.
                    continue
                if retryable:
                    raise OperatorError("NETWORK_UNCERTAIN_GET_RETRY_BUDGET_EXHAUSTED") from None
                raise OperatorError("NETWORK_UNCERTAIN_RERUN_EXACT_COMMAND") from None
            except http.client.HTTPException:
                raise OperatorError("HTTP_PROTOCOL_ERROR_NO_RETRY") from None


class Bundle:
    def __init__(self, args):
        self.files = PrivateFiles(args.private_root)
        self.company = args.company
        need(IDENT.fullmatch(self.company) is not None, "COMPANY_ALIAS_INVALID")
        need(SHA.fullmatch(args.controls_sha256) is not None, "CONTROLS_SEED_HASH_REQUIRED")
        manifest = self.files.json(args.controls_manifest, args.controls_sha256, 1024 * 1024)
        v4 = manifest.get("format") == "nxt-private-qbo-explicit-controls/v4"
        need(v4 or manifest.get("format") == "nxt-private-qbo-explicit-controls/v1", "CONTROL_MANIFEST_FORMAT_UNKNOWN")
        matches = [r for r in manifest["results"] if r["cohort_ref"] == self.company]
        need(len(matches) == 1, "COMPANY_NOT_UNIQUE_IN_MANIFEST")
        record = matches[0]
        self.template = self.files.json(record["preview_template_path"], record["sha256"], 4 * 1024 * 1024)
        acquisition = self.files.json(args.acquisition_manifest, manifest["verification_sha256"], 4 * 1024 * 1024)
        need(acquisition.get("format") == "nxt-private-queued-qbo-verification/v1", "ACQUISITION_MANIFEST_FORMAT_UNKNOWN")
        acquisition_stage = record["predecessor_stage_path"] if v4 else self.template["stage_path"]
        matches = [r for r in acquisition["results"] if r["cohort_ref"] == self.company and r["stage_path"] == acquisition_stage]
        need(len(matches) == 1, "ACQUISITION_BINDING_MISSING")
        acquired = matches[0]
        verification = acquired["verification"]
        # A stage may carry entries held for human review (SUSPENSE_REVIEW).
        # They are never posted; such a stage may only reach a local test copy.
        self.review_blockers = list(verification.get("review_blockers") or [])
        held_ok = "SUSPENSE_REVIEW" in self.review_blockers and isinstance(verification.get("quarantined_groups"), int) and verification["quarantined_groups"] > 0
        need(all(verification.get(k) is True for k in ("source_archives_unchanged", "independent_raw_row_equality", "all_source_rows_represented_exactly_once")) and
             (verification.get("quarantined_groups") == 0 or held_ok) and verification.get("source_unknown_amount_rows") == 0, "SOURCE_ROW_COMPLETENESS_NOT_VERIFIED")
        self.stage = self.files.path(self.template["stage_path"], exists=False)
        need(self.stage.is_dir(), "STAGE_DIRECTORY_MISSING")
        self.input = copy.deepcopy(self.template["input"])
        source_stage = self.files.path(acquisition_stage, exists=False)
        source_summary = self.files.json(source_stage / "summary.json", verification["stage_summary_sha256"], 1024 * 1024)
        if v4:
            summary = self._v4(manifest, record, verification)
            inventory = summary["native_chunks"]
            expected_entries, expected_rows = summary["native_entries"], summary["native_lines"]
        else:
            summary, inventory = source_summary, source_summary["backend_transport_chunks"]
            expected_entries, expected_rows = verification["backend_entries"], verification["backend_rows"]
        need(list(source_summary.get("review_blockers") or []) == self.review_blockers and
             source_summary.get("quarantined_groups", 0) == verification.get("quarantined_groups") == len({h["entry_start_row"] for h in source_summary.get("held_for_review", [])}), "SOURCE_STAGE_BLOCKED")
        need(source_summary.get("restricted_source_controls_passed") is True and not source_summary.get("missing_amount_source_rows") and
             (not source_summary.get("quarantined_groups") or held_ok) and source_summary["source_archive_sha256"] == self.input["source"]["archive_sha256"], "SOURCE_STAGE_BLOCKED")
        refs = self.template["local_transport_chunks"]
        need(refs == inventory and 0 < len(refs) <= 256, "CHUNK_INVENTORY_MISMATCH")
        self.source_archive = self.files.path(acquired["resolved_source_path"])
        archive_raw = self.files.read(self.source_archive, 64 * 1024 * 1024, source_summary["source_archive_sha256"])
        self._verify_archive_members(self.source_archive, self.input["source"])
        self.chunks, self.chunk_hashes, self.accounts, self.entries = [], [], {}, []
        local_bytes = 0
        # Multi-chunk books: refs are an ordered inventory (the stager writes chunk-000, chunk-001, ...).
        # Every chunk is loaded and bound before any trial-balance control is evaluated; _analysis()
        # below runs only once, after the final chunk, over the whole all-dates journal.
        need(len({r["file"] for r in refs}) == len(refs) and len({r["sha256"] for r in refs}) == len(refs), "CHUNK_INVENTORY_DUPLICATE")
        for ref in refs:
            raw = self.files.read(self.stage / ref["file"], MAX_CHUNK, ref["sha256"])
            need(len(raw) == ref["bytes"], "CHUNK_LENGTH_MISMATCH")
            chunk = parse(raw)
            canonical = chunk_bytes(chunk)
            need(len(canonical) <= MAX_CHUNK and len(chunk["entries"]) == ref["entries"] and (chunk["entries"] or chunk["accounts"]), "CHUNK_PAYLOAD_BOUND")
            local_bytes += len(canonical)
            need(local_bytes <= MAX_BOOK, "BOOK_SIZE_BOUND")
            self.chunks.append(chunk)
            self.chunk_hashes.append(sha(canonical))
            for account in chunk["accounts"]:
                need(account["id"] not in self.accounts, "DUPLICATE_ACCOUNT")
                self.accounts[account["id"]] = account
            self.entries.extend(chunk["entries"])
        self._metadata_context(v4)
        need(len(set(self.chunk_hashes)) == len(self.chunk_hashes), "DUPLICATE_CHUNK")
        need(len(self.entries) == expected_entries and sum(len(e["lines"]) for e in self.entries) == expected_rows, "SOURCE_ROW_COVERAGE_MISMATCH")
        need(self.input["source"]["journal_sha256"] == verification["journal_sha256"], "JOURNAL_HASH_MISMATCH")
        for ref in source_summary.get("artifact_refs", []):
            raw = self.files.read(source_stage / ref["file"], 64 * 1024 * 1024, ref["sha256"])
            need(len(raw) == ref["bytes"], "SOURCE_SIDECAR_LENGTH_MISMATCH")
        evidence = self.files.json(args.evidence, args.evidence_sha256, 1024 * 1024)
        self._evidence(evidence)
        self.expected_analysis = self._analysis()
        self.seed = {"company_id": self.company, "controls_sha256": args.controls_sha256,
                     "template_sha256": record["sha256"], "acquisition_sha256": manifest["verification_sha256"],
                     "stage_summary_sha256": verification["stage_summary_sha256"], "evidence_sha256": args.evidence_sha256,
                     "source_archive_sha256": sha(archive_raw), "journal_sha256": self.input["source"]["journal_sha256"],
                     "trial_balance_sha256": self.input["source"]["trial_balance_sha256"],
                     "chunk_transport_sha256": [r["sha256"] for r in refs], "chunk_store_sha256": self.chunk_hashes,
                     "native_stage_summary_sha256": record["v4_stage_summary_sha256"] if v4 else verification["stage_summary_sha256"],
                     "native_metadata_contract": "qbo-lexical-parent-context/v4" if v4 else "original-entry-and-row-metadata/v2",
                     "independent_metadata_equality_sha256": manifest["v4_context"]["independent_equality_sha256"] if v4 else None}

    def _v4(self, seed, record, verification):
        ctx = seed["v4_context"]
        need(ctx.get("contract") == "qbo-lexical-parent-context/v4" and ctx.get("tamper_tests_passed", 0) >= 12, "V4_CONTRACT_NOT_REVIEWED")
        manifest = self.files.json(ctx["stage_manifest_path"], ctx["stage_manifest_sha256"], 4 * 1024 * 1024)
        proof = self.files.json(ctx["independent_equality_path"], ctx["independent_equality_sha256"], 4 * 1024 * 1024)
        need(manifest.get("format") == "nxt-private-qbo-lexical-context-stage/v4" and proof.get("format") == "nxt-private-qbo-lexical-context-equality/v4" and
             manifest.get("source_verification_sha256") == seed["verification_sha256"] == proof.get("source_verification_sha256") and
             proof.get("stage_manifest_sha256") == ctx["stage_manifest_sha256"] and proof.get("decoder_sha256") == ctx["decoder_script_sha256"], "V4_PROOF_CHAIN_MISMATCH")
        mr = [r for r in manifest["results"] if r["cohort_ref"] == self.company and r["stage_path"] == record["stage_path"]]
        pr = [r for r in proof["results"] if r["cohort_ref"] == self.company and r["stage_path"] == record["stage_path"]]
        need(len(mr) == len(pr) == 1, "V4_COMPANY_PROOF_MISSING")
        m, p = mr[0], pr[0]
        need(m["predecessor_stage_path"] == record["predecessor_stage_path"] and m["predecessor_summary_sha256"] == record["predecessor_stage_summary_sha256"] == verification["stage_summary_sha256"] and
             m["summary_ref"]["sha256"] == record["v4_stage_summary_sha256"] == p["stage_summary_sha256"] and
             m["template_ref"]["sha256"] == record["sha256"] == p["template_sha256"] and
             sha(dump(p)) == record["v4_equality_result_sha256"], "V4_STAGE_OR_PREDECESSOR_BINDING_MISMATCH")
        need(p.get("status") == "lossless_v4_source_equality_passed" and all(p.get(k) is True for k in ("entry_metadata_unchanged", "exact_normalized_row_equality", "exact_reconstructed_metadata_equality", "original_xlsx_row_cell_equality")) and
             p.get("external_reference_resolution") is False and p.get("source_rows_omitted") == 0 and m.get("source_rows_omitted") == 0 and
             m.get("all_entry_metadata_unchanged") is True and m.get("normalized_records_unchanged") is True and m.get("external_source_references_added") == 0 and
             m.get("native_api_limit_unchanged_bytes") == MAX_BOOK and 0 < m.get("native_bytes", 0) <= MAX_BOOK, "V4_LOSSLESS_EQUALITY_NOT_PROVEN")
        need(m["source_archive_sha256"] == p["source_archive_sha256"] == record["source_archive_sha256"] == self.input["source"]["archive_sha256"] and
             m["native_lines"] == p["source_rows"] and m["native_entries"] == p["source_groups"], "V4_SOURCE_COVERAGE_MISMATCH")
        summary_path = self.stage / m["summary_ref"]["file"]
        summary = self.files.json(summary_path, record["v4_stage_summary_sha256"], 1024 * 1024)
        need(summary == {k: v for k, v in m.items() if k != "summary_ref"} and self.files.path(self.stage / m["template_ref"]["file"]) == self.files.path(record["preview_template_path"]), "V4_STAGE_SUMMARY_MISMATCH")
        return summary

    def _metadata_context(self, v4):
        inherited = ("format", "source_archive_sha256", "source_report_sha256", "source_report_member", "source_worksheet_name", "source_worksheet_file")
        for entry in self.entries:
            parent = entry.get("source_metadata", {})
            for line in entry["lines"]:
                meta = line.get("source_metadata", {})
                if not v4:
                    need(meta.get("format") != "qbo-row/v4", "V4_PROOF_CHAIN_REQUIRED")
                    continue
                need(set(meta) == {"format", "basis", "parent_row", "source_row", "raw_cells", "cell_columns"} and
                     meta["format"] == "qbo-row/v4" and meta["basis"] == "entry" and all(k in parent for k in inherited) and
                     parent["format"] == "qbo-report-row-provenance/v2" and parent["source_archive_sha256"] == self.input["source"]["archive_sha256"] and
                     parent["source_report_sha256"] == self.input["source"]["journal_sha256"], "V4_PARENT_CONTEXT_INVALID")
                start, subtotal, boundary = (parent[k] for k in ("source_start_row", "source_subtotal_row", "source_boundary_row"))
                need(all(type(n) is int for n in (start, subtotal, boundary, meta["parent_row"], meta["source_row"], line["source_row"])) and
                     start == meta["parent_row"] and start <= meta["source_row"] < subtotal == boundary and meta["source_row"] == line["source_row"], "V4_PARENT_ANCHOR_OR_RANGE_MISMATCH")
                need(isinstance(meta["raw_cells"], list) and meta["raw_cells"] and isinstance(meta["cell_columns"], (dict, list)) and meta["cell_columns"], "V4_ORIGINAL_CELL_FRAME_MISSING")
                columns = set()
                for cell in meta["raw_cells"]:
                    need(isinstance(cell, list) and len(cell) == 5 and isinstance(cell[0], str) and cell[0] not in columns and type(cell[1]) is bool and
                         all(value is None or isinstance(value, str) for value in cell[2:]), "V4_ORIGINAL_CELL_FRAME_INVALID")
                    columns.add(cell[0])

    @staticmethod
    def _verify_archive_members(path, source):
        found = {source["journal_sha256"]: 0, source["trial_balance_sha256"]: 0}
        try:
            with zipfile.ZipFile(path) as archive:
                names = archive.namelist()
                need(len(names) == len(set(names)) and len(names) <= 1000, "SOURCE_ARCHIVE_MEMBER_BOUND")
                size = 0
                for info in archive.infolist():
                    if info.is_dir():
                        continue
                    size += info.file_size
                    need(info.file_size <= 32 * 1024 * 1024 and size <= 256 * 1024 * 1024 and not info.flag_bits & 1, "SOURCE_ARCHIVE_EXPANSION_BOUND")
                    with archive.open(info) as handle:
                        raw = handle.read(32 * 1024 * 1024 + 1)
                    need(len(raw) == info.file_size, "SOURCE_ARCHIVE_MEMBER_LENGTH")
                    key = sha(raw)
                    if key in found:
                        found[key] += 1
        except (zipfile.BadZipFile, RuntimeError):
            raise OperatorError("SOURCE_ARCHIVE_INVALID") from None
        need(len(found) == 2 and all(n == 1 for n in found.values()), "SOURCE_REPORT_HASH_NOT_UNIQUE")

    def _evidence(self, e):
        need(e.get("format") == "nxt-company-book-operator-evidence/v1" and e.get("company_id") == self.company and
             e.get("company_name") == self.input["company"]["name"], "OPERATOR_EVIDENCE_IDENTITY_MISMATCH")
        source_id, currency = e.get("source_company_id"), e.get("currency")
        need(isinstance(source_id, str) and source_id.strip() == source_id and 0 < len(source_id) <= 200 and not any(ord(c) < 32 for c in source_id) and
             isinstance(currency, str) and re.fullmatch(r"[A-Z]{3}", currency), "OPERATOR_EVIDENCE_UNKNOWN")
        need(e["identity"].get("verified") is True and e["identity"].get("observed_source_company_id") == source_id and
             e["currency_observation"].get("verified") is True and e["currency_observation"].get("observed_currency") == currency and
             e["period_lock"].get("verified") is True, "OPERATOR_OBSERVATION_UNVERIFIED")
        for section in ("identity", "currency_observation", "period_lock"):
            artifact = e[section]["artifact"]
            self.files.read(artifact["file"], 16 * 1024 * 1024, artifact["sha256"])
        closed = e["period_lock"].get("closed_through")
        need(closed is None or valid_date(closed), "SOURCE_PERIOD_LOCK_UNKNOWN")
        self.input["company"].update(source_company_id=source_id, currency=currency, identity_verified=True, currency_verified=True)
        self.input["source"]["period_lock"] = {"verified": True, "closed_through": closed,
                                                 "evidence_sha256": e["period_lock"]["artifact"]["sha256"]}

    def _analysis(self):
        need(self.input["source"]["full_history"] is True and self.entries and 0 < len(self.accounts) <= 20000 and len(self.entries) <= 100000, "SOURCE_HISTORY_INCOMPLETE")
        totals = {key: [0, 0, 0] for key in self.accounts}
        # Derived year-end closes (multi-year books) are labelled entries with
        # no source row; they are tallied apart so source-row proofs stay exact.
        closes = {key: [0, 0, 0] for key in self.accounts}
        rows, ids, source_ids = set(), set(), set()
        debit = credit = lines = 0
        for e in self.entries:
            need(e["id"] not in ids and e["source_id"] not in source_ids and valid_date(e["date"]) and
                 self.input["source"]["period_start"] <= e["date"] <= self.input["source"]["period_end"], "SOURCE_ENTRY_ID_OR_DATE_INVALID")
            ids.add(e["id"]); source_ids.add(e["source_id"])
            ed = ec = 0
            derived_close = is_derived_close(e)
            for line in e["lines"]:
                key, row = line["account_id"], line["source_row"]
                if derived_close:
                    need(key in totals and row == 0 and is_derived_close(line), "SOURCE_LINE_IDENTITY_INVALID")
                else:
                    need(key in totals and type(row) is int and row > 0 and row not in rows, "SOURCE_LINE_IDENTITY_INVALID")
                    rows.add(row)
                d, c = cents(line["debit_cents"]), cents(line["credit_cents"])
                need(not (d and c), "SOURCE_LINE_DOUBLE_SIDED")
                tally = closes if derived_close else totals
                tally[key][0] += d; tally[key][1] += c; tally[key][2] += 1
                ed += d; ec += c; lines += 1
            need(ed == ec, "SOURCE_ENTRY_UNBALANCED")
            debit += ed; credit += ec
        need(lines <= 500000, "SOURCE_LINE_COUNT_BOUND")
        checks = {c["account_id"]: c for c in self.input["trial_balance"]}
        need(len(checks) == len(self.input["trial_balance"]) and set(checks) == set(totals), "SOURCE_CONTROL_ACCOUNT_COVERAGE")
        balances, used_tb_rows = [], set()
        independent = derived = held_accounts = held_net = 0
        for key in sorted(totals):
            sd, sc, count = totals[key]
            yd, yc, ycount = closes[key]
            d, c = sd + yd, sc + yc
            control = checks[key]
            net = d - c
            # Held-for-review entries are never posted; a trial-balance row they
            # carry is reconciled only through an explicit, labelled residual.
            held = control.get("held_residual")
            residual = 0
            if held is not None:
                rd, rc = cents(held["debit_cents"]), cents(held["credit_cents"])
                need(not (rd and rc) and rd != rc, "HELD_RESIDUAL_INVALID")
                residual = rd - rc
                held_accounts += 1; held_net += residual
            expected = net + residual
            need((cents(control["debit_cents"]), cents(control["credit_cents"])) == (max(expected, 0), max(-expected, 0)), "SOURCE_ACCOUNT_RECONCILIATION_MISMATCH")
            proof = basis(control.get("basis"))
            need(proof is not None, "SOURCE_CONTROL_PROVENANCE_MISSING")
            if proof["kind"] == "source_report_row":
                need(proof["report_sha256"] == self.input["source"]["trial_balance_sha256"] and type(proof["row"]) is int and proof["row"] > 0 and proof["row"] not in used_tb_rows, "SOURCE_TB_ROW_PROOF_MISMATCH")
                used_tb_rows.add(proof["row"]); independent += 1
            elif proof["kind"] == "derived_zero_after_year_end_close":
                need(proof["journal_sha256"] == self.input["source"]["journal_sha256"] and proof["source_row_count"] == count and
                     cents(proof["journal_debit_cents"], True) == sd and cents(proof["journal_credit_cents"], True) == sc and
                     proof["close_line_count"] == ycount > 0 and cents(proof["close_debit_cents"], True) == yd and cents(proof["close_credit_cents"], True) == yc and
                     expected == 0, "SOURCE_DERIVED_ZERO_PROOF_MISMATCH")
                derived += 1
            else:
                need(proof["journal_sha256"] == self.input["source"]["journal_sha256"] and proof["source_row_count"] == count and ycount == 0 and
                     cents(proof["journal_debit_cents"], True) == d and cents(proof["journal_credit_cents"], True) == c and expected == 0, "SOURCE_DERIVED_ZERO_PROOF_MISMATCH")
                derived += 1
            account = self.accounts[key]
            balances.append({"account_id": key, "name": account["name"], "kind": account["kind"],
                             "debit_cents": str(max(net, 0)), "credit_cents": str(max(-net, 0)),
                             "journal_debit_cents": str(d), "journal_credit_cents": str(c), "source_control_basis": proof})
        need(held_net == 0, "HELD_RESIDUAL_UNBALANCED")
        return {"accounts": len(totals), "entries": len(self.entries), "lines": lines,
                "total_debit_cents": str(debit), "total_credit_cents": str(credit), "balances": balances, "blockers": [],
                "classified_reports_ready": all(a["kind"] is not None for a in self.accounts.values()), "controls_passed": True,
                "independent_trial_balance_accounts": independent, "derived_zero_accounts": derived,
                **({"held_residual_accounts": held_accounts} if held_accounts else {}),
                "control_provenance_complete": True, "chart_of_accounts_complete": False}


DERIVED_CLOSE_FORMAT = "qbo-derived-year-end-close/v1"


def is_derived_close(row):
    meta = row.get("source_metadata")
    return isinstance(meta, dict) and meta.get("format") == DERIVED_CLOSE_FORMAT


def valid_date(value):
    try:
        return isinstance(value, str) and len(value) == 10 and dt.date.fromisoformat(value).isoformat() == value
    except ValueError:
        return False


def generic_json(value):
    if isinstance(value, dict):
        return {k: generic_json(value[k]) for k in sorted(value)}
    if isinstance(value, list):
        return [generic_json(v) for v in value]
    need(not isinstance(value, float), "METADATA_FLOAT_UNSUPPORTED_PRESERVE_XML_TEXT")
    if type(value) is int:
        need(-(2 ** 63) <= value <= 2 ** 64 - 1, "METADATA_INTEGER_BOUND")
    return value


def chunk_bytes(chunk):
    c = ordered(chunk, ["accounts", "entries"])
    c["accounts"] = [ordered(a, ["id", "name", "kind"]) for a in c["accounts"]]
    entries = []
    for entry in c["entries"]:
        e = ordered(entry, ["id", "date", "source_id", "memo", "lines", "source_metadata"], ["source_metadata"])
        e["lines"] = [ordered(l, ["account_id", "debit_cents", "credit_cents", "source_row", "memo", "source_metadata"], ["source_metadata"]) for l in e["lines"]]
        for row in [e] + e["lines"]:
            if row.get("source_metadata") is not None:
                row["source_metadata"] = generic_json(row["source_metadata"])
            else:
                row.pop("source_metadata", None)
        entries.append(e)
    c["entries"] = entries
    return dump(c, ordered=True)


def controls(response):
    c = response.get("controls", {})
    need(c.get("source_authoritative") is True and c.get("parallel_test") is True and c.get("live_cutover_allowed") is False, "SERVER_CONTROL_SCOPE_CHANGED")


class Runner:
    def __init__(self, bundle, client):
        self.bundle, self.client = bundle, client
        self.base = "/api/company-books/" + urllib.parse.quote(bundle.company, safe="")

    def preflight(self):
        state = self.client.request("GET", "/api/os/status")
        need(state.get("auth_required") is True, "TARGET_AUTH_NOT_ENFORCED")
        need(self.client.local or state.get("storage") == "supabase", "PRODUCTION_DURABILITY_NOT_SUPABASE")

    def _prepared(self, request_id):
        need(IDENT.fullmatch(request_id) is not None and request_id != "UNASSIGNED", "EXPLICIT_REQUEST_ID_REQUIRED")
        body = copy.deepcopy(self.bundle.input)
        body.update(request_id=request_id, expected_head=None, chunks=self.bundle.chunk_hashes)
        return body

    def _check_preview(self, response, body):
        controls(response)
        p = response["preview"]
        need(p.get("format") == "nxt-company-book-preview/v1" and p.get("company_id") == self.bundle.company and
             dump(p["input"]) == dump(body) and dump(p["analysis"]) == dump(self.bundle.expected_analysis), "SERVER_PREVIEW_DIFFERS_FROM_SOURCE")
        need(response.get("stage") == "preview_ready" and response.get("preview_id") == sha(preview_bytes(p)), "SERVER_PREVIEW_HASH_OR_STAGE_MISMATCH")
        return response["preview_id"]

    def preview(self, args):
        self.bundle.files.output_path(args.receipt)
        self.bundle.files.output_path(str(args.receipt) + ".intent.json")
        self.preflight()
        body = self._prepared(args.request_id)
        intent = {"format": "nxt-company-book-operator-intent/v1", "mode": "preview", "origin": self.client.origin,
                  "seed": self.bundle.seed, "request_id": args.request_id, "input_sha256": sha(dump(body))}
        self.bundle.files.write(str(args.receipt) + ".intent.json", intent, same_ok=True)
        for chunk, expected in zip(self.bundle.chunks, self.bundle.chunk_hashes):
            reply = self.client.request("POST", self.base + "/chunks", chunk)
            need(reply.get("sha256") == expected and reply.get("bytes") == len(chunk_bytes(chunk)), "SERVER_CHUNK_IDENTITY_MISMATCH")
        response = self.client.request("POST", self.base + "/imports/preview", body)
        pid = self._check_preview(response, body)
        reread = self.client.request("GET", self.base + "/imports/" + pid)
        need(dump(reread) == dump(response), "SERVER_PREVIEW_READBACK_MISMATCH")
        receipt = {"format": "nxt-company-book-operator-preview/v1", "origin": self.client.origin, "seed": self.bundle.seed,
                   "preview_id": pid, "request_id": args.request_id, "input_sha256": sha(dump(body)),
                   "analysis_sha256": sha(dump(self.bundle.expected_analysis)), "expected_head": None,
                   "source_authoritative": True, "live_cutover_allowed": False, "financial_posts_requested": 0}
        return self._receipt(args.receipt, receipt)

    def _reviewed(self, path, digest):
        receipt = self.bundle.files.json(path, digest, 1024 * 1024)
        need(receipt.get("format") == "nxt-company-book-operator-preview/v1" and receipt.get("seed") == self.bundle.seed and
             receipt.get("origin") == self.client.origin and receipt.get("expected_head") is None, "REVIEWED_RECEIPT_BINDING_MISMATCH")
        body = self._prepared(receipt["request_id"])
        need(receipt.get("input_sha256") == sha(dump(body)) and receipt.get("analysis_sha256") == sha(dump(self.bundle.expected_analysis)), "REVIEWED_SOURCE_CHANGED")
        response = self.client.request("GET", self.base + "/imports/" + receipt["preview_id"])
        need(self._check_preview(response, body) == receipt["preview_id"], "REVIEWED_PREVIEW_CHANGED")
        return receipt, response

    def commit(self, args):
        self.bundle.files.output_path(args.receipt)
        self.bundle.files.output_path(str(args.receipt) + ".intent.json")
        self.preflight()
        reviewed, _ = self._reviewed(args.preview_receipt, args.preview_receipt_sha256)
        need(args.approve_preview_id == reviewed["preview_id"], "EXACT_PREVIEW_APPROVAL_REQUIRED")
        need(IDENT.fullmatch(args.request_id) is not None and isinstance(args.reviewer_label, str) and args.reviewer_label.strip() and len(args.reviewer_label) <= 200, "EXPLICIT_COMMIT_METADATA_REQUIRED")
        body = {"preview_id": reviewed["preview_id"], "expected_head": None,
                "request_id": args.request_id, "reviewer_label": args.reviewer_label}
        self.bundle.files.write(str(args.receipt) + ".intent.json", {"format": "nxt-company-book-operator-intent/v1", "mode": "commit", "origin": self.client.origin,
                                "seed": self.bundle.seed, "preview_receipt_sha256": args.preview_receipt_sha256,
                                "command": body, "command_sha256": sha(dump(body))}, same_ok=True)
        response = self.client.request("POST", self.base + "/imports/commit", body)
        controls(response)
        h = response["head"]
        need(response.get("head_sha256") == sha(initial_head_bytes(h)) and h["company_id"] == self.bundle.company and h["preview_id"] == reviewed["preview_id"] and
             h["commit_request_id"] == args.request_id and h["reviewer_label"] == args.reviewer_label and h["previous_head"] is None and not h["journal_chunks"] and
             h.get("locked_through") == self.bundle.input["source"]["period_lock"]["closed_through"], "COMMIT_RECEIPT_BINDING_MISMATCH")
        result = {"format": "nxt-company-book-operator-commit/v1", "origin": self.client.origin, "seed": self.bundle.seed,
                  "preview_receipt_sha256": args.preview_receipt_sha256, "preview_receipt": str(self.bundle.files.path(args.preview_receipt)),
                  "preview_id": reviewed["preview_id"], "head_sha256": response["head_sha256"], "head": h,
                  "command_sha256": sha(dump(body)), "idempotent_replay": response.get("idempotent_replay") is True,
                  "source_authoritative": True, "live_cutover_allowed": False, "financial_posts_requested": 1,
                  "identity_limit": "Shared-key operator labels and source observations are not independently authenticated identities."}
        receipt_path = self.bundle.files.path(args.receipt, exists=False)
        if receipt_path.exists():
            original = self.bundle.files.json(receipt_path, maximum=1024 * 1024)
            need(type(original.get("idempotent_replay")) is bool, "IMMUTABLE_COMMIT_RECEIPT_DIFFERS")
            # A retry can change this transport observation, never the accepted
            # command, source binding, head, or already retained receipt bytes.
            comparable_original = {k: v for k, v in original.items() if k != "idempotent_replay"}
            comparable_result = {k: v for k, v in result.items() if k != "idempotent_replay"}
            need(dump(comparable_original) == dump(comparable_result), "IMMUTABLE_COMMIT_RECEIPT_DIFFERS")
            return self._receipt(args.receipt, original, same_ok=True)
        return self._receipt(args.receipt, result)

    def _native_suffix(self, rows):
        source = self.bundle.entries
        need(len(rows) >= len(source), "TARGET_ENTRY_COUNT_MISMATCH")
        for index, entry in enumerate(source):
            need(dump(rows[index]) == dump(entry), "TARGET_SOURCE_ROW_OR_METADATA_MISMATCH")
        extra = rows[len(source):]
        totals = {key: [0, 0] for key in self.bundle.accounts}
        seen = {entry["id"] for entry in source}
        lines = 0
        for entry in extra:
            meta = entry.get("source_metadata")
            need(isinstance(meta, dict) and meta.get("origin") == "native_adjustment", "NATIVE_ENTRY_PROVENANCE_MISSING")
            need(entry.get("id") not in seen and isinstance(entry.get("lines"), list) and len(entry["lines"]) >= 2, "NATIVE_ENTRY_LINES_INVALID")
            seen.add(entry["id"])
            debit = credit = 0
            for line in entry["lines"]:
                line_meta = line.get("source_metadata")
                need(line.get("source_row") == 0 and isinstance(line_meta, dict) and line_meta.get("origin") == "native_adjustment", "NATIVE_LINE_PROVENANCE_MISSING")
                key = line.get("account_id")
                need(key in totals, "NATIVE_LINE_UNKNOWN_ACCOUNT")
                amount_debit, amount_credit = cents(line["debit_cents"]), cents(line["credit_cents"])
                need((amount_debit > 0) ^ (amount_credit > 0), "NATIVE_LINE_SIDE_INVALID")
                totals[key][0] += amount_debit
                totals[key][1] += amount_credit
                debit += amount_debit
                credit += amount_credit
                lines += 1
            need(debit == credit and debit > 0, "NATIVE_ENTRY_UNBALANCED")
        return extra, totals, lines

    def verify(self, args):
        self.bundle.files.output_path(args.receipt)
        self.bundle.files.output_path(args.readback_dir)
        try:
            return self._verify_initial_import(args, allow_native=args.allow_native_adjustments)
        except OperatorError as exc:
            if str(exc).startswith("NETWORK_UNCERTAIN"):
                budget = "GET_RETRY_BUDGET_EXHAUSTED_" if "GET_RETRY_BUDGET_EXHAUSTED" in str(exc) else ""
                raise OperatorError("VERIFY_NETWORK_UNCERTAIN_" + budget + "PARTIAL_OUTPUT_IS_NOT_ACCEPTANCE_USE_NEW_DIRECTORY_AND_RECEIPT") from None
            raise

    def _verify_initial_import(self, args, allow_native=False):
        self.preflight()
        committed = self.bundle.files.json(args.commit_receipt, args.commit_receipt_sha256, 1024 * 1024)
        need(committed.get("format") == "nxt-company-book-operator-commit/v1" and committed.get("seed") == self.bundle.seed and committed.get("origin") == self.client.origin and
             committed.get("head_sha256") == sha(initial_head_bytes(committed["head"])), "COMMIT_RECEIPT_CHANGED")
        reviewed, preview = self._reviewed(committed["preview_receipt"], committed["preview_receipt_sha256"])
        need(reviewed["preview_id"] == committed["preview_id"], "COMMIT_PREVIEW_BINDING_MISMATCH")
        output = self.bundle.files.output_path(args.readback_dir)
        need(not output.exists(), "READBACK_DIRECTORY_EXISTS_PRESERVE_PARTIAL_OUTPUT_USE_NEW_DIRECTORY_AND_RECEIPT")
        output.mkdir(parents=True)
        saved, saved_bytes = [], 0
        def save(name, value):
            nonlocal saved_bytes
            raw = dump(value)
            saved_bytes += len(raw)
            need(saved_bytes <= 128 * 1024 * 1024, "READBACK_TOTAL_BOUND")
            digest = self.bundle.files.write(output / name, value)
            saved.append({"file": name, "sha256": digest, "bytes": len(raw)})
        detail = self.client.request("GET", self.base)
        controls(detail)
        head = detail.get("head")
        need(isinstance(head, dict) and detail.get("company_id") == self.bundle.company and
             head.get("company_id") == self.bundle.company and head.get("preview_id") == committed["preview_id"] and
             SHA.fullmatch(detail.get("head_sha256", "")), "TARGET_HEAD_MISMATCH")
        if not allow_native:
            need(detail["head_sha256"] == committed["head_sha256"] and
                 dump(head) == dump(committed["head"]) and
                 sha(initial_head_bytes(head)) == detail["head_sha256"], "TARGET_FULL_INITIAL_HEAD_RECEIPT_MISMATCH")
        save("head.json", detail); save("preview.json", preview)
        offset, collected, declared = 0, [], None
        for number in range(10000):
            page = self.client.request("GET", self.base + "/entries?" + urllib.parse.urlencode({"offset": offset, "limit": 200, "max_bytes": 1048576}))
            controls(page)
            need(page.get("company_id") == self.bundle.company and page.get("company") == self.bundle.input["company"] and
                 page.get("head") == head and page.get("offset") == offset and page.get("max_bytes") == 1048576, "ENTRY_SNAPSHOT_CHANGED")
            total = page.get("total")
            need(type(total) is int and total >= 0, "TARGET_ENTRY_COUNT_MISMATCH")
            if not allow_native:
                need(total == len(self.bundle.entries), "TARGET_ENTRY_COUNT_MISMATCH")
            if declared is None:
                declared = total
            need(total == declared, "TARGET_ENTRY_COUNT_MISMATCH")
            rows = page["rows"]
            need(isinstance(rows, list), "TARGET_ENTRY_COUNT_MISMATCH")
            if not allow_native:
                for entry in rows:
                    need(len(collected) < len(self.bundle.entries) and dump(entry) == dump(self.bundle.entries[len(collected)]), "TARGET_SOURCE_ROW_OR_METADATA_MISMATCH")
                    collected.append(entry)
            else:
                collected.extend(rows)
            save("entries-%05d.json" % number, page)
            if not page["has_more"]:
                need(page.get("next_offset") is None and len(collected) == total, "ENTRY_CONTINUATION_MISMATCH")
                break
            need(rows and page.get("next_offset") == offset + len(rows), "ENTRY_CONTINUATION_MISMATCH")
            offset = page["next_offset"]
        else:
            raise OperatorError("ENTRY_PAGE_BOUND")
        if allow_native:
            extra, native_totals, native_lines = self._native_suffix(collected)
            if not extra:
                need(detail["head_sha256"] == committed["head_sha256"] and dump(head) == dump(committed["head"]) and
                     sha(initial_head_bytes(head)) == detail["head_sha256"], "TARGET_FULL_INITIAL_HEAD_RECEIPT_MISMATCH")
            else:
                need(detail["head_sha256"] != committed["head_sha256"], "NATIVE_HEAD_STILL_INITIAL")
        else:
            extra, native_totals, native_lines = [], {key: [0, 0] for key in self.bundle.accounts}, 0
        seen = len(collected)
        balances, offset = [], 0
        baseline = {k: v for k, v in self.bundle.expected_analysis.items() if k != "balances"}
        native_debit = sum(pair[0] for pair in native_totals.values())
        native_credit = sum(pair[1] for pair in native_totals.values())
        unchanged = all(pair[0] == pair[1] for pair in native_totals.values())
        expected_delta = {"entries": len(extra), "lines": native_lines, "total_debit_cents": str(native_debit), "total_credit_cents": str(native_credit)}
        for number in range(1000):
            page = self.client.request("GET", self.base + "/trial-balance?" + urllib.parse.urlencode({"offset": offset, "limit": 200, "max_bytes": 1048576}))
            controls(page)
            need(page.get("company_id") == self.bundle.company and page.get("company") == self.bundle.input["company"] and
                 page.get("head") == head and page.get("source") == self.bundle.input["source"] and
                 page.get("source_baseline_analysis") == baseline and
                 page.get("offset") == offset and page.get("total") == len(self.bundle.accounts), "TARGET_BALANCE_SNAPSHOT_MISMATCH")
            if not extra:
                need(page.get("analysis") == baseline, "TARGET_BALANCE_SNAPSHOT_MISMATCH")
                need(page.get("native_delta") == {"entries": 0, "lines": 0, "total_debit_cents": "0", "total_credit_cents": "0"} and
                     page.get("reconciliation") == {"source_baseline_unchanged": True, "source_controls_passed": True, "combined_balances_match_source": True}, "TARGET_INITIAL_IMPORT_RECONCILIATION_MISMATCH")
            else:
                analysis = page.get("analysis")
                need(isinstance(analysis, dict) and analysis.get("entries") == baseline["entries"] + len(extra) and
                     analysis.get("lines") == baseline["lines"] + native_lines and
                     analysis.get("total_debit_cents") == str(int(baseline["total_debit_cents"]) + native_debit) and
                     analysis.get("total_credit_cents") == str(int(baseline["total_credit_cents"]) + native_credit), "TARGET_NATIVE_ANALYSIS_MISMATCH")
                reconciliation = page.get("reconciliation")
                need(page.get("native_delta") == expected_delta and isinstance(reconciliation, dict) and
                     reconciliation.get("source_baseline_unchanged") is True and reconciliation.get("source_controls_passed") is True and
                     reconciliation.get("combined_balances_match_source") is unchanged, "TARGET_NATIVE_DELTA_MISMATCH")
            for row in page["balances"]:
                index = len(balances)
                need(index < len(self.bundle.expected_analysis["balances"]), "EXTRA_TARGET_ACCOUNT")
                expected = self.bundle.expected_analysis["balances"][index]
                need(all(row.get(k) == expected[k] for k in ("account_id", "name", "kind", "source_control_basis")) and
                     all(row.get("source_" + k) == expected[k] for k in ("debit_cents", "credit_cents", "journal_debit_cents", "journal_credit_cents")), "TARGET_SOURCE_ACCOUNT_BALANCE_MISMATCH")
                added = native_totals[expected["account_id"]]
                journal_debit = cents(expected["journal_debit_cents"], True) + added[0]
                journal_credit = cents(expected["journal_credit_cents"], True) + added[1]
                net = journal_debit - journal_credit
                need(row.get("native_delta_debit_cents") == str(added[0]) and row.get("native_delta_credit_cents") == str(added[1]), "TARGET_NATIVE_DELTA_MISMATCH")
                if not extra:
                    need(all(row.get(k) == expected[k] for k in expected), "TARGET_ACCOUNT_BALANCE_MISMATCH")
                    need(added == [0, 0], "UNEXPECTED_NATIVE_BALANCE_DELTA")
                else:
                    need(row.get("journal_debit_cents") == str(journal_debit) and row.get("journal_credit_cents") == str(journal_credit) and
                         row.get("debit_cents") == str(max(net, 0)) and row.get("credit_cents") == str(max(-net, 0)), "TARGET_NATIVE_BALANCE_MISMATCH")
                balances.append(row)
            save("trial-balance-%05d.json" % number, page)
            if not page["has_more"]:
                need(page.get("next_offset") is None and len(balances) == page["total"], "BALANCE_CONTINUATION_MISMATCH")
                break
            need(page["balances"] and page.get("next_offset") == offset + len(page["balances"]), "BALANCE_CONTINUATION_MISMATCH")
            offset = page["next_offset"]
        else:
            raise OperatorError("BALANCE_PAGE_BOUND")
        after = self.client.request("GET", self.base)
        controls(after)
        need(after.get("company_id") == self.bundle.company and after.get("head_sha256") == detail["head_sha256"] and after.get("head") == head, "HEAD_CHANGED_DURING_VERIFY")
        save("head-after.json", after)
        manifest = {"format": "nxt-company-book-private-readback/v1", "company_id": self.bundle.company, "seed": self.bundle.seed,
                    "source_preview_id": committed["preview_id"], "head_sha256": detail["head_sha256"], "files": saved,
                    "source_entries_exact": len(self.bundle.entries), "source_lines_exact": self.bundle.expected_analysis["lines"],
                    "source_accounts_exact": len(balances), "target_entries_checked": seen,
                    "native_entries_recomputed": len(extra), "native_lines_recomputed": native_lines,
                    "verification_policy": "independent_native_delta" if extra else "strict_initial_import_no_native_adjustments",
                    "source_authoritative": True, "live_cutover_allowed": False}
        if not extra:
            manifest.pop("native_entries_recomputed")
            manifest.pop("native_lines_recomputed")
        digest = self.bundle.files.write(output / "manifest.json", manifest)
        receipt = {"format": "nxt-company-book-operator-verification/v1", "origin": self.client.origin,
                            "seed": self.bundle.seed, "commit_receipt_sha256": args.commit_receipt_sha256,
                            "head_sha256": detail["head_sha256"], "readback_manifest_sha256": digest,
                            "source_entries_exact": len(self.bundle.entries), "source_lines_exact": self.bundle.expected_analysis["lines"],
                            "source_accounts_exact": len(balances), "target_entries_checked": seen,
                            "verification_policy": manifest["verification_policy"],
                            "source_authoritative": True, "live_cutover_allowed": False, "financial_posts_requested": 0}
        if extra:
            receipt.update(native_entries_recomputed=len(extra), native_lines_recomputed=native_lines,
                           native_debit_cents=str(native_debit), native_credit_cents=str(native_credit))
        return self._receipt(args.receipt, receipt)

    def _receipt(self, path, value, same_ok=False):
        digest = self.bundle.files.write(path, value, same_ok=same_ok)
        return {"mode": value["format"].split("-")[-1].split("/")[0], "company_id": self.bundle.company,
                "receipt_sha256": digest, "source_entries": len(self.bundle.entries),
                "source_lines": self.bundle.expected_analysis["lines"], "source_accounts": len(self.bundle.accounts),
                "source_authoritative": True, "live_cutover_allowed": False}


def main(argv=None):
    parser = SafeParser(description=__doc__)
    parser.add_argument("mode", choices=("preview", "commit", "verify"))
    parser.add_argument("--target", choices=("production", "test-copy"), default="production")
    for name in ("private-root", "controls-manifest", "controls-sha256", "acquisition-manifest", "company", "evidence", "evidence-sha256", "receipt"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--origin")
    for name in ("request-id", "preview-receipt", "preview-receipt-sha256", "approve-preview-id", "reviewer-label", "commit-receipt", "commit-receipt-sha256", "readback-dir"):
        parser.add_argument("--" + name)
    parser.add_argument("--allow-native-adjustments", action="store_true")
    args = parser.parse_args(argv)
    try:
        required = {"preview": ["request_id"], "commit": ["request_id", "preview_receipt", "preview_receipt_sha256", "approve_preview_id", "reviewer_label"],
                    "verify": ["commit_receipt", "commit_receipt_sha256", "readback_dir"]}[args.mode]
        need(all(getattr(args, key) for key in required), "MODE_ARGUMENTS_MISSING")
        if args.target == "test-copy":
            # A persistent local test copy: its own key and origin, never production.
            args.origin = args.origin or os.environ.get("NXT_TEST_COMPANY_BOOKS_ORIGIN") or "http://127.0.0.1:3917"
            need(origin(args.origin)[1], "TEST_COPY_TARGET_MUST_BE_LOCAL")
            token = os.environ.get("NXT_TEST_COMPANY_BOOKS_KEY")
        else:
            need(bool(args.origin), "MODE_ARGUMENTS_MISSING")
            token = os.environ.get("NXT_COMPANY_BOOKS_KEY")
        client = Transport(args.origin, token)
        bundle = Bundle(args)
        # Review blockers (SUSPENSE_REVIEW, OPENING_BALANCE_UNRECONCILED) never
        # reach production; only a local test copy until a human resolves them.
        need(client.local or not bundle.review_blockers, "SUSPENSE_REVIEW_BLOCKS_PRODUCTION" if "SUSPENSE_REVIEW" in bundle.review_blockers else "REVIEW_BLOCKERS_BLOCK_PRODUCTION")
        result = getattr(Runner(bundle, client), args.mode)(args)
        print(json.dumps(result, sort_keys=True))
        return 0
    except OperatorError as exc:
        print(json.dumps({"ok": False, "error": str(exc)}), file=sys.stderr)
        return 2
    except (KeyError, TypeError, ValueError, OSError, RecursionError):
        print(json.dumps({"ok": False, "error": "INPUT_SCHEMA_OR_PRIVATE_FILE_INVALID"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
