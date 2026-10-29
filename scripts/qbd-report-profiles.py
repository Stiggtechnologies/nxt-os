#!/usr/bin/env python3
"""QuickBooks Desktop report profiles: P&L, balance sheet, AR/AP aging.

Read-only. Stdlib only. Three jobs:

  derive   accounts.xml + trial_balance.xml (qbXML AccountQuery and
           GeneralSummaryReport TrialBalance) -> P&L and balance sheet by
           QBD AccountType, with a balance check. This is NXT's derived view.
  parse    any qbXML GeneralSummaryReport (ProfitAndLossStandard,
           BalanceSheetStandard) or AgingReport (ARAgingSummary,
           APAgingSummary) response -> rows, columns and totals.
  compare  derived profile vs a parsed QBD report: per-section totals.
  requests print the qbXML requests to export those reports from QBD.

Vendor P&L / BS / aging report files have not been seen yet, so `parse`
and `compare` are tested on synthetic qbXML only (verified: false).

Usage:
  qbd-report-profiles.py derive ACCOUNTS.xml TRIAL_BALANCE.xml
  qbd-report-profiles.py parse REPORT.xml
  qbd-report-profiles.py compare ACCOUNTS.xml TRIAL_BALANCE.xml REPORT.xml
  qbd-report-profiles.py requests
"""
import json, sys
from decimal import Decimal
import xml.etree.ElementTree as ET

PL = {"Income": "income", "OtherIncome": "other_income", "CostOfGoodsSold": "cost_of_goods_sold",
      "Expense": "expense", "OtherExpense": "other_expense"}
BS = {"Bank": "asset", "AccountsReceivable": "asset", "OtherCurrentAsset": "asset", "FixedAsset": "asset",
      "OtherAsset": "asset", "AccountsPayable": "liability", "CreditCard": "liability",
      "OtherCurrentLiability": "liability", "LongTermLiability": "liability", "Equity": "equity"}
REQUESTS = {
    "profit_and_loss": ("GeneralSummaryReportQueryRq", "ProfitAndLossStandard"),
    "balance_sheet": ("GeneralSummaryReportQueryRq", "BalanceSheetStandard"),
    "ar_aging": ("AgingReportQueryRq", "ARAgingSummary"),
    "ap_aging": ("AgingReportQueryRq", "APAgingSummary"),
}


class ProfileError(Exception):
    pass


def _xml(data):
    if b"<!DOCTYPE" in data[:2048] or b"<!ENTITY" in data:
        raise ProfileError("DTD/entities are refused")
    return ET.fromstring(data)


def _d(v):
    try:
        return Decimal(v.replace(",", "")) if v not in (None, "") else Decimal(0)
    except Exception:
        raise ProfileError(f"bad amount {v!r}")


def accounts(data):
    out = {}
    for a in _xml(data).iter("AccountRet"):
        out[a.findtext("FullName") or a.findtext("Name")] = a.findtext("AccountType")
    return out


def trial_balance(data):
    rep = parse_report(data)
    rows = {}
    for r in rep["rows"]:
        if r["kind"] == "data":
            v = r["values"]
            rows[r["label"]] = _d(v.get("2")) - _d(v.get("3"))  # debit - credit
    return rows, rep


def derive(acc_data, tb_data):
    types = accounts(acc_data)
    tb, rep = trial_balance(tb_data)
    sec = {k: Decimal(0) for k in list(PL.values()) + ["asset", "liability", "equity"]}
    unmapped = []
    for name, net in tb.items():
        t = types.get(name)
        if t in PL:
            sec[PL[t]] += net
        elif t in BS:
            sec[BS[t]] += net
        else:
            unmapped.append({"account": name, "type": t, "net": str(net)})
    income = -(sec["income"] + sec["other_income"])
    cogs, exp = sec["cost_of_goods_sold"], sec["expense"] + sec["other_expense"]
    net_income = income - cogs - exp
    assets, liab, eq = sec["asset"], -sec["liability"], -sec["equity"]
    return {
        "format": "nxt-qbd-report-profile/v1", "basis": "derived_from_trial_balance_and_account_types",
        "trial_balance_total_row": next((r["values"] for r in rep["rows"] if r["kind"] == "total"), None),
        "profit_and_loss": {"income": str(-sec["income"]), "other_income": str(-sec["other_income"]),
                            "cost_of_goods_sold": str(cogs), "gross_profit": str(-sec["income"] - cogs),
                            "expense": str(sec["expense"]), "other_expense": str(sec["other_expense"]),
                            "net_income": str(net_income)},
        "balance_sheet": {"assets": str(assets), "liabilities": str(liab), "equity_before_net_income": str(eq),
                          "net_income": str(net_income), "liabilities_and_equity": str(liab + eq + net_income),
                          "balanced": assets == liab + eq + net_income},
        "unmapped_accounts": unmapped,
        "aging": "not derivable from a trial balance; needs ARAgingSummary/APAgingSummary export",
    }


def parse_report(data):
    root = _xml(data)
    ret = root.find(".//ReportRet")
    if ret is None:
        raise ProfileError("no ReportRet")
    cols = {c.get("colID"): " ".join(t.get("value", "") for t in c.findall("ColTitle")).strip()
            for c in ret.findall("ColDesc")}
    rows = []
    kinds = {"DataRow": "data", "TextRow": "text", "SubtotalRow": "subtotal", "TotalRow": "total"}
    rd = ret.find("ReportData")
    for el in (list(rd) if rd is not None else []):
        k = kinds.get(el.tag)
        if not k:
            continue
        vals = {c.get("colID"): c.get("value") for c in el.findall("ColData")}
        rdat = el.find("RowData")
        label = (rdat.get("value") if rdat is not None else None) or el.get("value") or vals.get("1") or ""
        rows.append({"kind": k, "row": int(el.get("rowNumber") or 0), "label": label,
                     "row_type": rdat.get("rowType") if rdat is not None else None, "values": vals})
    return {"title": ret.findtext("ReportTitle"), "subtitle": ret.findtext("ReportSubtitle"),
            "basis": ret.findtext("ReportBasis"), "columns": cols, "rows": rows,
            "totals": [r for r in rows if r["kind"] in ("subtotal", "total")]}


# Labels QBD prints on P&L / BS total rows, mapped to derived keys.
TOTAL_LABELS = {"total income": ("profit_and_loss", "income"), "gross profit": ("profit_and_loss", "gross_profit"),
                "total cogs": ("profit_and_loss", "cost_of_goods_sold"), "total expense": ("profit_and_loss", "expense"),
                "net income": ("profit_and_loss", "net_income"), "total assets": ("balance_sheet", "assets"),
                "total liabilities": ("balance_sheet", "liabilities"),
                "total liabilities & equity": ("balance_sheet", "liabilities_and_equity")}


def compare(profile, report):
    out = []
    for r in report["totals"]:
        key = TOTAL_LABELS.get(r["label"].strip().lower())
        if not key:
            continue
        amt = next((v for c, v in sorted(r["values"].items()) if c != "1" and v), None)
        mine = Decimal(profile[key[0]][key[1]])
        out.append({"label": r["label"], "report": amt, "derived": str(mine),
                    "match": amt is not None and _d(amt) == mine})
    return {"verified": False, "compared": out, "all_match": bool(out) and all(x["match"] for x in out)}


def aging_totals(report):
    """Per-bucket totals from an AgingReport TotalRow, keyed by column title."""
    t = next((r for r in report["rows"] if r["kind"] == "total"), None)
    return {report["columns"].get(c, c): v for c, v in (t["values"].items() if t else []) if c != "1"}


def request_xml(name):
    rq, typ = REQUESTS[name]
    tag = "GeneralSummaryReportType" if rq.startswith("General") else "AgingReportType"
    return (f'<?xml version="1.0"?><?qbxml version="13.0"?><QBXML><QBXMLMsgsRq onError="stopOnError">'
            f'<{rq}><{tag}>{typ}</{tag}></{rq}></QBXMLMsgsRq></QBXML>')


def main(a):
    rd = lambda p: open(p, "rb").read()
    try:
        if a[:1] == ["derive"] and len(a) == 3:
            out = derive(rd(a[1]), rd(a[2]))
        elif a[:1] == ["parse"] and len(a) == 2:
            out = parse_report(rd(a[1]))
        elif a[:1] == ["compare"] and len(a) == 4:
            out = compare(derive(rd(a[1]), rd(a[2])), parse_report(rd(a[3])))
        elif a[:1] == ["requests"]:
            out = {k: request_xml(k) for k in REQUESTS}
        else:
            print(__doc__); return 2
    except (OSError, ProfileError, ET.ParseError) as e:
        print(f"error: {e}", file=sys.stderr); return 1
    json.dump(out, sys.stdout, indent=1); print()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
