"""Dry-run coverage for the reconstructed bank-statement loader. No network and no real books."""
import importlib.util
import io
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "reconstructed_books_load",
    Path(__file__).parents[1] / "scripts" / "reconstructed-books-load.py",
)
loader = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(loader)


def cell(ref, value):
    text = str(value).replace("&", "&amp;").replace("<", "&lt;")
    return f'<c r="{ref}" t="inlineStr"><is><t>{text}</t></is></c>'


def sheet(rows):
    body = []
    for index, row in enumerate(rows, 1):
        cells = "".join(cell(f"{chr(65 + column)}{index}", value) for column, value in enumerate(row))
        body.append(f'<row r="{index}">{cells}</row>')
    xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f'<sheetData>{"".join(body)}</sheetData></worksheet>'
    )
    return xml


def workbook(rows_by_sheet):
    names = list(rows_by_sheet)
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr("[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>')
        sheets = "".join(
            f'<sheet name="{name}" sheetId="{index}" r:id="rId{index}"/>'
            for index, name in enumerate(names, 1)
        )
        archive.writestr(
            "xl/workbook.xml",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
            'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
            f"<sheets>{sheets}</sheets></workbook>",
        )
        rels = "".join(
            f'<Relationship Id="rId{index}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{index}.xml"/>'
            for index in range(1, len(names) + 1)
        )
        archive.writestr(
            "xl/_rels/workbook.xml.rels",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f"{rels}</Relationships>",
        )
        for index, name in enumerate(names, 1):
            archive.writestr(f"xl/worksheets/sheet{index}.xml", sheet(rows_by_sheet[name]))
    return buffer.getvalue()


def package(balanced=True, approved=True):
    credit = "5.00" if balanced else "4.00"
    approval = "Approved by Orville Davis, 2026-11-26 2:31 AM MT" if approved else "not approved"
    return workbook({
        "README_Import_Memo": [
            ["Field", "Value"],
            ["Company", "Example Holdco (synthetic reconstruction fixture)"],
            ["Status", "DRAFT / RECONSTRUCTED from a bank statement."],
            ["Approval", approval],
            ["Fiscal year-end", "PROVISIONAL Dec 31."],
            ["Opening / closing bank", "Opening January 31 2026: 0.00. Closing January 31 2026: -5.00."],
        ],
        "Chart_of_Accounts": [
            ["code", "name", "type", "class_"],
            ["1000", "Bank", "Bank", "Asset"],
            ["5110", "Bank charges", "Expense", "Expense"],
            ["1990", "TO REVIEW suspense", "Suspense", "Asset"],
        ],
        "Trial_Balance": [
            ["code", "name", "debit", "credit"],
            ["1000", "Bank", "0.00", "5.00"],
            ["5110", "Bank charges", "5.00", "0.00"],
            ["1990", "TO REVIEW suspense", "0.00", "0.00"],
        ],
        "Bank_Transactions": [
            ["seq", "date", "desc", "amount", "statement", "account"],
            ["1", "2026-01-15", "Monthly fee", "-5.00", "RBC-2310 stmt 2026-01-31", "5110"],
        ],
        "Journal": [
            ["entry", "date", "memo", "account", "debit", "credit"],
            ["BK-0001", "2026-01-15", "Monthly fee", "1000", "0.00", credit],
            ["BK-0001", "2026-01-15", "Monthly fee", "5110", "5.00", "0.00"],
        ],
    })


class LoaderTest(unittest.TestCase):
    def write(self, raw):
        folder = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: folder.joinpath("package.xlsx").unlink(missing_ok=True))
        target = folder / "package.xlsx"
        target.write_bytes(raw)
        return target

    def test_dry_run_balances_and_labels_the_book(self):
        target = self.write(package())
        with mock.patch.object(loader.urllib.request, "urlopen", side_effect=AssertionError("dry-run must not open a socket")):
            summary = loader.build(target)
        self.assertTrue(summary["summary"]["balanced"])
        self.assertEqual(summary["summary"]["book_posture"], "DRAFT / RECONSTRUCTED")
        self.assertEqual(summary["summary"]["source_system"], "reconstructed_bank_statements")
        self.assertEqual(summary["summary"]["accounts"], 3)
        self.assertEqual(summary["summary"]["entries"], 1)
        self.assertEqual(summary["summary"]["lines"], 2)
        self.assertEqual(summary["summary"]["approver"], "Orville Davis")
        self.assertTrue(summary["summary"]["provisional_year_end"])
        self.assertEqual(summary["summary"]["year_end"], "2026-12-31")
        self.assertEqual(summary["summary"]["to_review_accounts"], ["1990"])
        self.assertNotIn("quickbooks", json.dumps(summary).lower())
        self.assertEqual(summary["preview"]["source"]["system"], "reconstructed_bank_statements")
        self.assertNotIn("period_lock", summary["preview"]["source"])
        code = loader.main(["--package", str(target)])
        self.assertEqual(code, 0)

    def test_unbalanced_package_is_rejected(self):
        target = self.write(package(balanced=False))
        with self.assertRaises(loader.PackageError) as caught:
            loader.build(target)
        self.assertIn("unbalanced", str(caught.exception).lower())

    def test_unapproved_package_is_rejected(self):
        target = self.write(package(approved=False))
        with self.assertRaises(loader.PackageError) as caught:
            loader.build(target)
        self.assertIn("approv", str(caught.exception).lower())

    def test_commit_requires_credentials_and_refuses_production_before_any_request(self):
        target = self.write(package())
        with mock.patch.object(loader.urllib.request, "urlopen", side_effect=AssertionError("no request")):
            with mock.patch.dict("os.environ", {}, clear=True):
                with self.assertRaises(loader.PackageError) as caught:
                    loader.main(["--package", str(target), "--commit", "--base-url", "http://127.0.0.1:9"])
        self.assertIn("NXT_COMPANY_BOOKS_KEY", str(caught.exception))
        with mock.patch.object(loader.urllib.request, "urlopen", side_effect=AssertionError("no request")):
            with mock.patch.dict("os.environ", {"NXT_COMPANY_BOOKS_KEY": "test-key"}, clear=True):
                with self.assertRaises(loader.PackageError) as caught:
                    loader.main(["--package", str(target), "--commit", "--base-url", "https://os.nxtcorporation.ca"])
        self.assertIn("os.nxtcorporation.ca", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
