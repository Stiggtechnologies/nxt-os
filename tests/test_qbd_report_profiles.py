"""Synthetic qbXML only; no client data."""
import importlib.util, os, unittest
HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("qrp", os.path.join(HERE, "..", "scripts", "qbd-report-profiles.py"))
qrp = importlib.util.module_from_spec(spec); spec.loader.exec_module(qrp)

ACC = b"""<QBXML><QBXMLMsgsRs><AccountQueryRs>
<AccountRet><FullName>Bank</FullName><AccountType>Bank</AccountType></AccountRet>
<AccountRet><FullName>AP</FullName><AccountType>AccountsPayable</AccountType></AccountRet>
<AccountRet><FullName>Capital</FullName><AccountType>Equity</AccountType></AccountRet>
<AccountRet><FullName>Sales</FullName><AccountType>Income</AccountType></AccountRet>
<AccountRet><FullName>Rent</FullName><AccountType>Expense</AccountType></AccountRet>
</AccountQueryRs></QBXMLMsgsRs></QBXML>"""

def row(n, label, dr="", cr=""):
    return (f'<DataRow rowNumber="{n}"><RowData rowType="account" value="{label}"/><ColData colID="1" value="{label}"/>'
            + (f'<ColData colID="2" value="{dr}"/>' if dr else "") + (f'<ColData colID="3" value="{cr}"/>' if cr else "")
            + "</DataRow>")

TB = ("<QBXML><QBXMLMsgsRs><GeneralSummaryReportQueryRs><ReportRet><ReportTitle>Trial Balance</ReportTitle>"
      '<ColDesc colID="1"><ColTitle titleRow="1"/></ColDesc><ColDesc colID="2"><ColTitle titleRow="1" value="Debit"/></ColDesc>'
      '<ColDesc colID="3"><ColTitle titleRow="1" value="Credit"/></ColDesc><ReportData>'
      + row(1, "Bank", dr="1,500.00") + row(2, "AP", cr="200.00") + row(3, "Capital", cr="1000.00")
      + row(4, "Sales", cr="700.00") + row(5, "Rent", dr="400.00")
      + '<TotalRow rowNumber="6"><ColData colID="1" value="TOTAL"/><ColData colID="2" value="1900.00"/><ColData colID="3" value="1900.00"/></TotalRow>'
      "</ReportData></ReportRet></GeneralSummaryReportQueryRs></QBXMLMsgsRs></QBXML>").encode()

PNL = (b"<QBXML><QBXMLMsgsRs><GeneralSummaryReportQueryRs><ReportRet><ReportTitle>Profit &amp; Loss</ReportTitle>"
       b'<ColDesc colID="1"><ColTitle titleRow="1"/></ColDesc><ColDesc colID="2"><ColTitle titleRow="1" value="TOTAL"/></ColDesc><ReportData>'
       b'<TextRow rowNumber="1" value="Income"/>'
       b'<SubtotalRow rowNumber="2"><RowData rowType="label" value="Total Income"/><ColData colID="1" value="Total Income"/><ColData colID="2" value="700.00"/></SubtotalRow>'
       b'<TotalRow rowNumber="3"><RowData rowType="label" value="Net Income"/><ColData colID="1" value="Net Income"/><ColData colID="2" value="300.00"/></TotalRow>'
       b"</ReportData></ReportRet></GeneralSummaryReportQueryRs></QBXMLMsgsRs></QBXML>")

AGING = (b"<QBXML><QBXMLMsgsRs><AgingReportQueryRs><ReportRet><ReportTitle>A/R Aging Summary</ReportTitle>"
         b'<ColDesc colID="1"><ColTitle titleRow="1"/></ColDesc><ColDesc colID="2"><ColTitle titleRow="1" value="Current"/></ColDesc>'
         b'<ColDesc colID="3"><ColTitle titleRow="1" value="1 - 30"/></ColDesc><ColDesc colID="4"><ColTitle titleRow="1" value="TOTAL"/></ColDesc><ReportData>'
         b'<DataRow rowNumber="1"><RowData rowType="name" value="Synthetic Co"/><ColData colID="2" value="50.00"/><ColData colID="3" value="25.00"/><ColData colID="4" value="75.00"/></DataRow>'
         b'<TotalRow rowNumber="2"><ColData colID="1" value="TOTAL"/><ColData colID="2" value="50.00"/><ColData colID="3" value="25.00"/><ColData colID="4" value="75.00"/></TotalRow>'
         b"</ReportData></ReportRet></AgingReportQueryRs></QBXMLMsgsRs></QBXML>")

class T(unittest.TestCase):
    def test_derive_balances(self):
        d = qrp.derive(ACC, TB)
        self.assertEqual(d["profit_and_loss"]["net_income"], "300.00")
        self.assertEqual(d["balance_sheet"]["assets"], "1500.00")
        self.assertTrue(d["balance_sheet"]["balanced"])
        self.assertEqual(d["unmapped_accounts"], [])

    def test_compare_pnl_unverified(self):
        c = qrp.compare(qrp.derive(ACC, TB), qrp.parse_report(PNL))
        self.assertFalse(c["verified"]); self.assertTrue(c["all_match"]); self.assertEqual(len(c["compared"]), 2)

    def test_aging_buckets(self):
        self.assertEqual(qrp.aging_totals(qrp.parse_report(AGING)), {"Current": "50.00", "1 - 30": "25.00", "TOTAL": "75.00"})

    def test_unmapped_and_dtd(self):
        d = qrp.derive(ACC, TB.replace(b'value="Rent"', b'value="Mystery"'))
        self.assertEqual(len(d["unmapped_accounts"]), 1)
        with self.assertRaises(qrp.ProfileError):
            qrp.parse_report(b'<!DOCTYPE x [<!ENTITY a "b">]><x/>')

    def test_requests(self):
        self.assertIn("APAgingSummary", qrp.request_xml("ap_aging"))

if __name__ == "__main__":
    unittest.main()
