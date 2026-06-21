import beancount.loader
from beancount.core import realization

LEDGER_PATH = r"G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount"
entries, errors, options_map = beancount.loader.load_file(LEDGER_PATH)

accounts = set()
for entry in entries:
    if type(entry).__name__ == 'Transaction':
        for posting in entry.postings:
            if posting.account.startswith('Assets:Retirement') or posting.account.startswith('Assets:Investments'):
                accounts.add(posting.account)

print("Target Accounts found:")
for a in sorted(list(accounts)):
    print(a)
