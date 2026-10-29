#!/usr/bin/env python3
"""Autonomous client path: detect -> import -> test-copy verify -> gated production.

One idempotent, resumable orchestrator per client. It reads a client source folder
or ZIP, detects the export format, runs the matching repository importer, loads and
verifies the staged books on a TEST origin, and promotes to production ONLY under
gate G5: zero blockers AND real (non-synthetic) evidence.json AND a passing test-copy
verify AND an explicit --production-origin with NXT_COMPANY_BOOKS_KEY present.

Never deletes or modifies source files (sources are hashed before and after), never
signs in to cloud services, never files or sends anything externally. Unknown or
not-yet-automated formats become blockers, never guesses. Ledger: <work>/status.json.
"""
import argparse, hashlib, json, os, re, subprocess, sys, zipfile
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
PY = sys.executable
LEDGER_FORMAT = "nxt-autonomous-client-path/v1"
QBXML_TO_REPORTS = os.environ.get("NXT_QBXML_TO_REPORTS", "/workspace/nxt-migration/qbxml_to_reports.py")
REVIEWER = "autonomous-client-path:G5-zero-blockers"


def sha_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def source_fingerprint(src):
    src = Path(src)
    files = [src] if src.is_file() else sorted(p for p in src.rglob("*") if p.is_file())
    return {str(p.relative_to(src) if src.is_dir() else p.name): sha_file(p) for p in files}


def names_in(src):
    src = Path(src)
    if src.is_dir():
        return [str(p.relative_to(src)).replace("\\", "/") for p in src.rglob("*") if p.is_file()]
    if zipfile.is_zipfile(src):
        with zipfile.ZipFile(src) as z:
            return [i.filename for i in z.infolist() if not i.is_dir()]
    return [src.name]


def detect(src):
    """Return (format, detail). Order matters; ambiguous => 'ambiguous' blocker."""
    src = Path(src)
    names = names_in(src)
    low = [n.lower() for n in names]
    base = {n.rsplit("/", 1)[-1] for n in low}
    hits = []
    if src.is_dir() and {"trial_balance.xml", "journal.xml"} <= base:
        hits.append("qbd_qbxml")
    if any(n.endswith(".xlsx") for n in low) and any(re.search(r"(trial.?balance|journal|general.?ledger)", n) for n in low):
        hits.append("qbo_export")
    if any(n.endswith(".csv") for n in low) and any("dtmax" in n or "dt max" in n or "t1" in n.rsplit("/", 1)[-1] for n in low):
        hits.append("dtmax_csv")
    if any(n.endswith(".ac") or n.endswith(".ac_") or "caseware" in n for n in low):
        hits.append("caseware")
    if any(n.endswith(".cor") for n in low):
        hits.append("webtax_cor")
    if any("docusign" in n or n.endswith("summary.pdf") or "certificate" in n for n in low) and src.suffix.lower() == ".zip":
        hits.append("docusign_zip")
    if any(n.endswith(".xls") for n in low) and any("payroll" in n or "pay" in n for n in low):
        hits.append("payroll_xls")
    if not hits:
        return "unknown", {"files": len(names)}
    if len(hits) > 1:
        return "ambiguous", {"candidates": hits}
    return hits[0], {"files": len(names)}


# Formats with a reader in the repo whose company-books path is not yet automated.
HANDOFF = {
    "dtmax_csv": "scripts/read-dtmax-export.py (tax staging; no company-books path)",
    "caseware": "scripts/read-caseware-engagement.py (engagement reader; no company-books path)",
    "webtax_cor": "src/source_import/webtax_cor.rs (server import parser)",
    "docusign_zip": "src/source_import/docusign.rs (server import parser)",
    "payroll_xls": "src/source_import/payroll.rs (server import parser)",
}


class Run:
    def __init__(self, a):
        self.a = a
        self.work = Path(a.work).resolve()
        self.work.mkdir(parents=True, exist_ok=True)
        self.path = self.work / "status.json"
        self.s = json.loads(self.path.read_text()) if self.path.exists() else {
            "format": LEDGER_FORMAT, "client": a.client, "steps": {}, "blockers": []}
        self.s["blockers"] = []

    def save(self):
        self.s["updated_at"] = datetime.now(timezone.utc).isoformat()
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.s, indent=2, sort_keys=True))
        tmp.replace(self.path)

    def done(self, step, key):
        st = self.s["steps"].get(step)
        return st and st.get("status") == "ok" and st.get("key") == key

    def mark(self, step, status, key=None, **kw):
        self.s["steps"][step] = {"status": status, "key": key, "at": datetime.now(timezone.utc).isoformat(), **kw}
        self.save()

    def block(self, code, **kw):
        self.s["blockers"].append({"code": code, **kw})
        self.s["state"] = "blocked"
        self.save()
        return 2

    def sh(self, step, cmd, env=None):
        r = subprocess.run(cmd, capture_output=True, text=True, env=env)
        tail = (r.stderr or r.stdout)[-800:]
        return r.returncode, r.stdout, tail


def operator(run, origin, key, out, mode, extra):
    e = {k: v for k, v in os.environ.items() if k not in ("NXT_COMPANY_BOOKS_KEY",)}
    if key:
        e["NXT_COMPANY_BOOKS_KEY"] = key
    st = out
    cmd = [PY, str(HERE / "company-books-operator.py"), mode, "--origin", origin, "--private-root", str(st),
           "--controls-manifest", "controls.json", "--controls-sha256", sha_file(st / "controls.json"),
           "--acquisition-manifest", "acquisition.json", "--company", run.a.client,
           "--evidence", "evidence.json", "--evidence-sha256", sha_file(st / "evidence.json")] + extra
    return run.sh(mode, cmd, e)


def promote(run, label, origin, key, st):
    """preview -> commit -> verify against one origin. Each sub-step resumable by receipt."""
    rid = f"acp-{label}-{run.s['source_key'][:16]}"
    pre, com, ver = f"{label}-preview.json", f"{label}-commit.json", f"{label}-verify.json"
    if not (st / pre).exists():
        rc, _, t = operator(run, origin, key, st, "preview", ["--receipt", pre, "--request-id", rid])
        if rc:
            return run.block(f"{label.upper()}_PREVIEW_FAILED", detail=t)
    pid = json.loads((st / pre).read_text())["preview_id"]
    if not (st / com).exists():
        rc, _, t = operator(run, origin, key, st, "commit", ["--receipt", com, "--request-id", rid,
            "--preview-receipt", pre, "--preview-receipt-sha256", sha_file(st / pre),
            "--approve-preview-id", pid, "--reviewer-label", REVIEWER])
        if rc:
            return run.block(f"{label.upper()}_COMMIT_FAILED", detail=t, note="rerun resumes with the same request id")
    n = sum(1 for p in st.glob(f"{label}-verify*.json")) + 1
    rc, _, t = operator(run, origin, key, st, "verify", ["--receipt", f"{label}-verify-{n}.json",
        "--commit-receipt", com, "--commit-receipt-sha256", sha_file(st / com),
        "--readback-dir", f"{label}-readback-{n}"])
    if rc:
        return run.block(f"{label.upper()}_VERIFY_FAILED", detail=t)
    run.mark(f"{label}_verify", "ok", run.s["source_key"], receipt=f"{label}-verify-{n}.json")
    return 0


def evidence_is_real(st):
    ev = st / "evidence.json"
    if not ev.exists():
        return False, "EVIDENCE_MISSING"
    try:
        e = json.loads(ev.read_text())
    except ValueError:
        return False, "EVIDENCE_UNREADABLE"
    if e.get("synthetic") is True or "synthetic" in str(e.get("format", "")) or e.get("source_company_id") == "synthetic-fixture":
        return False, "EVIDENCE_SYNTHETIC"
    for k in ("identity", "currency_observation", "period_lock"):
        art = (e.get(k) or {}).get("artifact") or {}
        f = art.get("file")
        if not (e.get(k) or {}).get("verified") or not f or not (st / f).exists():
            return False, f"EVIDENCE_{k.upper()}_UNVERIFIED"
        if sha_file(st / f) != art.get("sha256"):
            return False, f"EVIDENCE_{k.upper()}_HASH_MISMATCH"
        try:
            obs = json.loads((st / f).read_text())
        except (OSError, ValueError):
            obs = None
        if isinstance(obs, dict) and (obs.get("synthetic") is True or "synthetic" in str(obs.get("format", ""))):
            return False, "EVIDENCE_SYNTHETIC"
    return True, None


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--client", required=True, help="company id / cohort ref")
    p.add_argument("--source", required=True, help="client source folder or ZIP (read-only)")
    p.add_argument("--work", required=True, help="private work dir (outside Git); holds status.json")
    p.add_argument("--evidence", help="real evidence.json + its artifacts dir to copy into stage")
    p.add_argument("--test-origin", help="TEST copy origin (key: NXT_TEST_COMPANY_BOOKS_KEY)")
    p.add_argument("--production-origin", help="production origin (key: NXT_COMPANY_BOOKS_KEY)")
    p.add_argument("--date-order", default="auto", choices=["auto", "dmy", "mdy"])
    a = p.parse_args(argv)
    run = Run(a)
    src = Path(a.source).resolve()
    if not src.exists():
        return run.block("SOURCE_MISSING")
    if run.work == src or src in run.work.parents:
        return run.block("WORK_DIR_INSIDE_SOURCE")
    before = source_fingerprint(src)
    key = hashlib.sha256(json.dumps(before, sort_keys=True).encode()).hexdigest()
    if run.s.get("source_key") not in (None, key):
        run.s["previous_source_keys"] = run.s.get("previous_source_keys", []) + [run.s["source_key"]]
        run.s["steps"] = {}
    run.s["source_key"] = key
    try:
        return pipeline(run, src, key, a)
    finally:
        if source_fingerprint(src) != before:
            run.block("SOURCE_CHANGED_DURING_RUN")


def pipeline(run, src, key, a):
    fmt, detail = detect(src)
    run.s["detected_format"] = fmt
    run.mark("detect", "ok", key, format=fmt, detail=detail)
    if fmt in ("unknown", "ambiguous"):
        return run.block("FORMAT_" + fmt.upper(), detail=detail)
    if fmt in HANDOFF:
        return run.block("FORMAT_NOT_AUTOMATED_FOR_COMPANY_BOOKS", format=fmt, reader=HANDOFF[fmt])

    # Normalize to a QBO-style report ZIP.
    reports = run.work / "reports.zip"
    if fmt == "qbd_qbxml":
        if not run.done("qbxml_to_reports", key):
            if not Path(QBXML_TO_REPORTS).exists():
                return run.block("QBXML_CONVERTER_MISSING", path=QBXML_TO_REPORTS)
            # converter writes into its input dir; run on a copy so the source is never touched
            import shutil
            cp = run.work / "qbxml-copy"
            if cp.exists():
                shutil.rmtree(cp)
            shutil.copytree(src, cp)
            rc, out, t = run.sh("qbxml", [PY, QBXML_TO_REPORTS, str(cp)])
            if rc or not (cp / "source-reports.zip").exists():
                return run.block("QBXML_CONVERSION_FAILED", detail=t)
            (cp / "source-reports.zip").replace(reports)
            run.mark("qbxml_to_reports", "ok", key, stats=out.strip()[-500:])
    else:
        if not run.done("package", key):
            if src.is_file() and zipfile.is_zipfile(src):
                reports.write_bytes(src.read_bytes())
            else:
                with zipfile.ZipFile(reports, "w", zipfile.ZIP_DEFLATED) as z:
                    files = [src] if src.is_file() else sorted(src.rglob("*.xlsx"))
                    for f in files:
                        z.write(f, f.name)
            run.mark("package", "ok", key)

    # 1. inspect
    insp = run.work / f"inspect-{key[:12]}"
    if not run.done("inspect", key):
        rc, _, t = run.sh("inspect", [PY, str(HERE / "inspect-qbo-export.py"), "--source", str(reports), "--out", str(insp), "--date-order", a.date_order])
        rep = next(insp.glob("*.json"), None) if insp.exists() else None
        blockers = json.loads(rep.read_text()).get("blockers", []) if rep else [{"code": "INSPECT_NO_REPORT", "detail": t}]
        if rc or blockers:
            run.mark("inspect", "blocked", key, blockers=blockers)
            return run.block("INSPECT_BLOCKERS", blockers=blockers)
        run.mark("inspect", "ok", key)

    # 2. stage
    st = run.work / f"stage-{key[:12]}"
    if not run.done("stage", key):
        rc, out, t = run.sh("stage", [PY, str(HERE / "stage-qbo-journal.py"), "--source", str(reports), "--out", str(st), "--cohort", a.client, "--date-order", a.date_order])
        if rc or not (st / "controls.json").exists():
            return run.block("STAGE_FAILED", detail=t)
        run.mark("stage", "ok", key)

    # 3. evidence (real only; G5)
    if a.evidence and not (st / "evidence.json").exists():
        import shutil
        ev = Path(a.evidence)
        for f in ([ev] if ev.is_file() else []) + ([x for x in ev.parent.iterdir() if x.is_file() and x != ev] if ev.is_file() else []):
            if f.suffix == ".json" and not (st / f.name).exists():
                shutil.copy2(f, st / f.name)
    ok, why = evidence_is_real(st)
    if not ok:
        return run.block(why, note="supply real observed evidence via --evidence; synthetic evidence never promotes")
    run.mark("evidence", "ok", key)

    # 4. test copy load + verify
    if not a.test_origin:
        return run.block("TEST_ORIGIN_REQUIRED")
    if not run.done("test_verify", key):
        rc = promote(run, "test", a.test_origin, os.environ.get("NXT_TEST_COMPANY_BOOKS_KEY"), st)
        if rc:
            return rc

    # 5. G5: zero blockers => production
    if run.s["blockers"]:
        return 2
    if not a.production_origin:
        run.s["state"] = "test_verified_awaiting_production_origin"
        run.save()
        return 0
    if not os.environ.get("NXT_COMPANY_BOOKS_KEY"):
        return run.block("NXT_COMPANY_BOOKS_KEY_REQUIRED")
    if not run.done("production_verify", key):
        rc = promote(run, "production", a.production_origin, os.environ["NXT_COMPANY_BOOKS_KEY"], st)
        if rc:
            return rc
    run.s["state"] = "production_verified"
    run.save()
    return 0


if __name__ == "__main__":
    sys.exit(main())
