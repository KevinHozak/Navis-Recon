import beancount.loader
from beancount.core import realization

LEDGER_PATH = r"G:\My Drive\Navis-Protocol\Facilities\Vault\Ledger\Sovereign.beancount"

entries, errors, options_map = beancount.loader.load_file(LEDGER_PATH)
root = realization.realize(entries)

def get_balance(node):
    balance = realization.compute_balance(node)
    return round(float(balance.get_currency_units("USD")), 2) if "USD" in balance else 0.0

total_assets = get_balance(realization.get_or_create(root, "Assets"))
total_liabilities = get_balance(realization.get_or_create(root, "Liabilities"))
net_worth = total_assets + total_liabilities  # Liabilities are negative in Beancount usually? Let's check

print("Assets:", total_assets)
print("Liabilities:", total_liabilities)
print("Net Worth:", total_assets + total_liabilities)
