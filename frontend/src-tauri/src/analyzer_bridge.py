"""Local-only statement analysis. Native Rust owns all import mutations."""
import argparse
from datetime import datetime
from decimal import Decimal, InvalidOperation
import json
from pathlib import Path
import re
import subprocess
import tempfile


def emit(kind, **fields):
    print(json.dumps(dict(type=kind, **fields)), flush=True)


def parse_date(value):
    value = re.sub(r"(\d+)(st|nd|rd|th)", r"\1", value.strip())
    # Strip only abbreviated-month punctuation before numeric dotted-date
    # normalization; otherwise "Oct. 25 2026" becomes "Oct/ 25 2026".
    value = re.sub(r'\b(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\.',
                   r'\1', value, flags=re.IGNORECASE)
    for fmt in ("%m/%d/%Y", "%m/%d/%y", "%B %d, %Y", "%b %d, %Y",
                "%B %d %Y", "%b %d %Y", "%A, %B %d, %Y",
                "%Y-%m-%d", "%Y/%m/%d", "%Y年%m月"):
        try:
            return datetime.strptime(value.replace('.', '/'), fmt).strftime('%Y-%m-%d')
        except ValueError:
            pass
    return None


def rule_date(rule, text):
    for pattern in rule.get('date_patterns', []):
        matches = list(re.finditer(pattern, text, re.IGNORECASE))
        if not matches:
            continue
        selection = rule.get('date_selection', 'first')
        match = matches[0]
        if selection.startswith('nearest:'):
            label = text.casefold().find(selection.split(':', 1)[1].casefold())
            if label >= 0:
                match = min(matches, key=lambda m: abs(m.start() - label))
        elif selection.startswith('index:'):
            index = int(selection.split(':', 1)[1])
            if not 0 <= index < len(matches):
                raise ValueError('Date selection is outside matched dates')
            match = matches[index]
        # Existing rules capture the date after labels or a period's start date.
        # Parsing group zero loses those matches or chooses the wrong period end.
        candidates = [parse_date(g) for g in match.groups() if g]
        candidates = list(dict.fromkeys(d for d in candidates if d))
        if not match.groups():
            candidates = [d for d in [parse_date(match.group(0))] if d]
        if len(candidates) == 1:
            return candidates[0]
        if len(candidates) > 1:
            raise ValueError('Date pattern captures multiple different dates')
    return None


def local_amount(rule, text, currency):
    extraction = rule.get('local_extraction')
    if not isinstance(extraction, dict):
        raise ValueError('Rule/date matched; add a local_extraction amount_pattern and category. External AI is disabled.')
    pattern = extraction.get('amount_pattern', '')
    category = extraction.get('category', '')
    if extraction.get('currency') != currency:
        raise ValueError('Local extraction currency must match the configured ledger unit')
    if not pattern or not re.fullmatch(r'(Expenses|Income)(:[A-Z][A-Za-z0-9-]*)+', category):
        raise ValueError('Local extraction needs an explicit amount pattern and valid category')
    matches = list(re.finditer(pattern, text, re.IGNORECASE))
    if len(matches) != 1 or len(matches[0].groups()) != 1:
        raise ValueError('Local amount pattern must match exactly one amount with one capture group')
    token = matches[0].group(1).strip()
    if not re.fullmatch(r'-?(?:\d+|\d{1,3}(?:,\d{3})+)(?:\.\d{1,18})?', token):
        raise ValueError('Local amount is not an exact decimal')
    try:
        amount = Decimal(token.replace(',', ''))
    except InvalidOperation:
        raise ValueError('Local amount is invalid') from None
    if not amount.is_finite() or abs(amount) > Decimal('1000000000000000'):
        raise ValueError('Local amount is outside supported bounds')
    return format(amount, 'f'), category


def analyze_text(filename, text, rules, currency='USD'):
    normalized = ' '.join(text.split()) + ' ' + filename
    for rule in rules:
        if not all(k.casefold() in normalized.casefold() for k in rule['keywords']):
            continue
        date = rule_date(rule, normalized)
        if not date:
            continue
        if rule.get('review_only_reason'):
            raise ValueError(f"{rule['name']} ({date}): {rule['review_only_reason']}")
        amount, category = local_amount(rule, normalized, currency)
        return dict(filename=filename, new_name=rule['target_template'].format(date=date),
                    rule=rule['name'], date=date, amount=amount, category=category,
                    is_cleared=False, was_encrypted=False)
    return None


def page_text(pdf, reader, args):
    texts = [(page.extract_text() or '') for page in reader.pages[:2]]
    if args.ocr and any(not text.strip() for text in texts):
        with tempfile.TemporaryDirectory(prefix='recon-local-ocr-') as folder:
            for index, text in enumerate(texts):
                if text.strip():
                    continue
                prefix = str(Path(folder) / 'page')
                flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
                subprocess.run([args.pdftoppm, '-f', str(index + 1), '-l', str(index + 1),
                                '-r', '150', '-png', '-singlefile', str(pdf), prefix],
                               check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               timeout=45, creationflags=flags)
                result = subprocess.run([args.tesseract, prefix + '.png', 'stdout', '-l', 'eng'],
                                        check=True, capture_output=True, text=True, encoding='utf-8',
                                        timeout=45, creationflags=flags)
                texts[index] = result.stdout
    return ' '.join(texts)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--inbox', required=True)
    parser.add_argument('--rules', required=True)
    parser.add_argument('--currency', required=True)
    parser.add_argument('--ocr', action='store_true')
    parser.add_argument('--tesseract', default='tesseract')
    parser.add_argument('--pdftoppm', default='pdftoppm')
    args = parser.parse_args()
    try:
        from pypdf import PdfReader
        inbox = Path(args.inbox)
        if not inbox.is_absolute() or not inbox.is_dir():
            raise ValueError('Configured analyzer inbox is unavailable or not absolute')
        rules = json.loads(Path(args.rules).read_text(encoding='utf-8-sig'))['rules']
        if not isinstance(rules, list):
            raise ValueError('Rules must contain a rules array')
    except Exception:
        emit('error', message='Local analyzer initialization failed; check pypdf, configured inbox and rules file.')
        return 1
    for pdf in sorted(inbox.glob('*.pdf')):
        emit('log', message='Analyzing configured inbox PDF', filename=pdf.name)
        try:
            if pdf.is_symlink() or not pdf.resolve().is_relative_to(inbox.resolve()):
                raise ValueError('Source is outside configured inbox')
            reader = PdfReader(pdf)
            if reader.is_encrypted:
                raise ValueError('Encrypted PDF needs a separately decrypted local copy')
            text = page_text(pdf, reader, args)
            if not text.strip():
                raise ValueError('PDF has no extractable text; create a local OCR copy or review manually. External AI is disabled.')
            candidate = analyze_text(pdf.name, text, rules, args.currency)
            if candidate:
                emit('match', **candidate)
            else:
                emit('unmatched', filename=pdf.name, message='No institution/date rule matched; review rule coverage.')
        except ValueError as error:
            emit('unmatched', filename=pdf.name, message=str(error))
        except (FileNotFoundError, subprocess.SubprocessError):
            emit('error', filename=pdf.name, message='Local OCR failed; check configured Poppler/Tesseract executables. No external service was used.')
        except Exception:
            emit('error', filename=pdf.name, message='PDF or rule could not be analyzed locally')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
