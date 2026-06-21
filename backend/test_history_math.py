import beancount.loader
from beancount.core.data import Transaction
from collections import defaultdict
from datetime import datetime
import json

LEDGER_PATH = r"G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount"
entries, errors, options_map = beancount.loader.load_file(LEDGER_PATH)

balances = defaultdict(float)
monthly_snapshots = {}

for entry in entries:
    if isinstance(entry, Transaction):
        month_str = entry.date.strftime("%Y-%m")
        for posting in entry.postings:
            amt = float(posting.units.number) if posting.units else 0.0
            balances[posting.account] += amt
        
        nw = sum(v for k, v in balances.items() if k.startswith('Assets') or k.startswith('Liabilities'))
        hsa_total = sum(v for k, v in balances.items() if k.startswith('Assets:Retirement:HSA'))
        
        monthly_snapshots[month_str] = {
            "name": month_str,
            "Net Worth": round(nw, 2),
            "401k (K)": round(balances.get('Assets:Retirement:401k:K', 0.0), 2),
            "IRA (J)": round(balances.get('Assets:Retirement:IRA:J', 0.0), 2),
            "IRA (K)": round(balances.get('Assets:Retirement:IRA:K', 0.0), 2),
            "Stocks": round(balances.get('Assets:Investments:Stocks', 0.0), 2),
            "J-Stocks": round(balances.get('Assets:Investments:JStocks', 0.0), 2),
            "HSA": round(hsa_total, 2)
        }

sorted_months = sorted(monthly_snapshots.keys())
# Fill gaps
results = []
last_snap = None
if sorted_months:
    start_y, start_m = map(int, sorted_months[0].split('-'))
    end_y, end_m = map(int, sorted_months[-1].split('-'))
    
    curr_y, curr_m = start_y, start_m
    while (curr_y < end_y) or (curr_y == end_y and curr_m <= end_m):
        ms = f"{curr_y}-{curr_m:02d}"
        if ms in monthly_snapshots:
            last_snap = monthly_snapshots[ms]
        else:
            if last_snap:
                last_snap = last_snap.copy()
                last_snap["name"] = ms
        
        if last_snap:
            # We want to format 'name' as short month 'Jan' or '01'
            last_snap_copy = last_snap.copy()
            # Let's keep name as "YYYY-MM" and the frontend can parse it.
            results.append(last_snap_copy)
            
        curr_m += 1
        if curr_m > 12:
            curr_m = 1
            curr_y += 1

print(json.dumps(results[-12:], indent=2))
