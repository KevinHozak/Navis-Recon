"""Synthetic local-only analyzer regression; no credentials or household files."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

BRIDGE = Path(__file__).parent / 'src' / 'analyzer_bridge.py'
spec = importlib.util.spec_from_file_location('bridge', BRIDGE)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


def rule(**updates):
    value = dict(name='Acceptance Bank', keywords=['Acceptance Bank'],
                 date_patterns=[r'Statement Date:\s+(\d{2}/\d{2}/\d{4})'],
                 target_template='Acceptance Bank {date}.pdf',
                 local_extraction=dict(amount_pattern=r'Total:\s+(-?[\d,.]+)',
                                       category='Expenses:Acceptance', currency='USD'))
    value.update(updates)
    return value


def pdf_fixture(path):
    from pypdf import PdfWriter
    from pypdf.generic import DictionaryObject, NameObject, DecodedStreamObject
    writer = PdfWriter()
    page = writer.add_blank_page(width=612, height=792)
    font = DictionaryObject({NameObject('/Type'): NameObject('/Font'),
                             NameObject('/Subtype'): NameObject('/Type1'),
                             NameObject('/BaseFont'): NameObject('/Helvetica')})
    page[NameObject('/Resources')] = DictionaryObject({NameObject('/Font'): DictionaryObject({NameObject('/F1'): writer._add_object(font)})})
    stream = DecodedStreamObject()
    stream.set_data(b'BT /F1 12 Tf 50 740 Td (Acceptance Bank Statement Date: 09/30/2026 Total: 12.34) Tj ET')
    page[NameObject('/Contents')] = writer._add_object(stream)
    with path.open('wb') as output:
        writer.write(output)


class AnalyzerTests(unittest.TestCase):
    def test_captured_label_and_period_end(self):
        self.assertEqual(bridge.rule_date(rule(), 'Statement Date: 09/30/2026'), '2026-09-30')
        r = rule(date_patterns=[r'Period: \d{2}/\d{2}/\d{4} to (\d{2}/\d{2}/\d{4})'])
        self.assertEqual(bridge.rule_date(r, 'Period: 09/01/2026 to 09/30/2026'), '2026-09-30')

    def test_date_selection_and_ambiguity(self):
        r = rule(date_patterns=[r'(\d{2}/\d{2}/\d{4})'], date_selection='index:1')
        self.assertEqual(bridge.rule_date(r, '09/01/2026 09/30/2026'), '2026-09-30')
        r['date_selection'] = 'nearest:Closing'
        self.assertEqual(bridge.rule_date(r, '09/01/2026  Other                 Closing 09/30/2026'), '2026-09-30')
        r['date_patterns'] = [r'(\d{2}/\d{2}/\d{4}) to (\d{2}/\d{2}/\d{4})']
        with self.assertRaises(ValueError):
            bridge.rule_date(r, '09/01/2026 to 09/30/2026')

    def test_exact_amount_and_currency(self):
        amount, category = bridge.local_amount(rule(), 'Total: -1,234.000000000000000001', 'USD')
        self.assertEqual(amount, '-1234.000000000000000001')
        self.assertEqual(category, 'Expenses:Acceptance')
        for text in ('Total: 1.23 Total: 4.56', 'Total: 1,23', 'Nothing here'):
            with self.assertRaises(ValueError):
                bridge.local_amount(rule(), text, 'USD')
        with self.assertRaises(ValueError):
            bridge.local_amount(rule(), 'Total: 12.34', 'TWD')

    def test_missing_local_rule_never_guesses(self):
        r = rule()
        del r['local_extraction']
        with self.assertRaisesRegex(ValueError, 'External AI is disabled'):
            bridge.analyze_text('test.pdf', 'Acceptance Bank Statement Date: 09/30/2026', [r])

    def test_split_payroll_rule_is_review_only_even_with_an_amount_mapping(self):
        r = rule(review_only_reason='Split payroll requires multi-posting review; do not map net pay to salary.')
        with self.assertRaisesRegex(ValueError, r'Acceptance Bank \(2026-09-30\): Split payroll'):
            bridge.analyze_text('payroll.pdf', 'Acceptance Bank Statement Date: 09/30/2026 Total: 12.34', [r])

    def test_ocr_is_local_bounded_and_only_for_empty_pages(self):
        reader = SimpleNamespace(pages=[SimpleNamespace(extract_text=lambda: ''),
                                       SimpleNamespace(extract_text=lambda: 'existing text')])
        args = SimpleNamespace(ocr=True, pdftoppm='local-poppler', tesseract='local-tesseract')
        with patch.object(bridge.subprocess, 'run', side_effect=[SimpleNamespace(), SimpleNamespace(stdout='Local OCR text')]) as run:
            text = bridge.page_text(Path('statement.pdf'), reader, args)
        self.assertEqual(text, 'Local OCR text existing text')
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args_list[0].args[0][0], 'local-poppler')
        self.assertEqual(run.call_args_list[1].args[0][0], 'local-tesseract')
        self.assertTrue(all(call.kwargs['timeout'] == 45 for call in run.call_args_list))

    def test_real_pdf_cli_is_read_only_and_uses_configured_inbox(self):
        with tempfile.TemporaryDirectory(prefix='recon-analyzer-') as folder:
            root = Path(folder)
            inbox = root / 'inbox with spaces'
            inbox.mkdir()
            pdf = inbox / 'statement.pdf'
            pdf_fixture(pdf)
            before = hashlib.sha256(pdf.read_bytes()).hexdigest()
            rules = root / 'rules.json'
            rules.write_text(json.dumps({'rules': [rule()]}), encoding='utf-8')
            env = dict(os.environ, GEMINI_API_KEY='must-not-be-used', GOOGLE_API_KEY='must-not-be-used')
            result = subprocess.run([sys.executable, '-c', BRIDGE.read_text(), '--inbox', str(inbox), '--rules', str(rules), '--currency', 'USD'], capture_output=True, text=True, timeout=30, env=env, cwd=root)
            self.assertEqual(result.returncode, 0, result.stderr)
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            match = next(row for row in rows if row['type'] == 'match')
            self.assertEqual(match['amount'], '12.34')
            self.assertEqual(match['new_name'], 'Acceptance Bank 2026-09-30.pdf')
            self.assertEqual(before, hashlib.sha256(pdf.read_bytes()).hexdigest())
            self.assertEqual(set(p.name for p in root.iterdir()), {'inbox with spaces', 'rules.json'})

    def test_image_only_pdf_reports_ocr_requirement(self):
        from pypdf import PdfWriter
        with tempfile.TemporaryDirectory(prefix='recon-scan-') as folder:
            root = Path(folder)
            writer = PdfWriter()
            writer.add_blank_page(width=612, height=792)
            with (root / 'scan.pdf').open('wb') as output:
                writer.write(output)
            rules = root / 'rules.json'
            rules.write_text(json.dumps({'rules': [rule()]}), encoding='utf-8')
            result = subprocess.run([sys.executable, '-c', BRIDGE.read_text(), '--inbox', str(root), '--rules', str(rules), '--currency', 'USD'], capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0)
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            self.assertFalse(any(row['type'] == 'match' for row in rows))
            self.assertIn('local OCR', rows[-1]['message'])


if __name__ == '__main__':
    unittest.main()
