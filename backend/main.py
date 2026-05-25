from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
import os
import beancount.loader
from beancount.core import realization

app = FastAPI(title="Family Ledger API")

# Allow CORS for local Vite frontend
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173", "http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

LEDGER_PATH = r"G:\My Drive\Navis-Protocol\Facilities\Vault\Ledger\Sovereign.beancount"

class LedgerApproveRequest(BaseModel):
    line_num: int
    category: str

@app.get("/api/ledger/summary")
async def get_ledger_summary():
    try:
        entries, errors, options_map = beancount.loader.load_file(LEDGER_PATH)
        root = realization.realize(entries)
        
        def get_usd_balance(node):
            balance = realization.compute_balance(node)
            amount = balance.get_currency_units("USD")
            return float(amount.number) if amount else 0.0

        def get_leaf_balances(node, strip_prefix=""):
            leaves = {}
            if node is None:
                return leaves
            if not node.values(): # It's a leaf
                bal = get_usd_balance(node)
                if bal != 0:
                    name = node.account.replace(strip_prefix, "") if strip_prefix else node.account
                    leaves[name] = bal
            else:
                for child in node.values():
                    leaves.update(get_leaf_balances(child, strip_prefix))
            return leaves

        assets_node = realization.get_or_create(root, "Assets")
        liabilities_node = realization.get_or_create(root, "Liabilities")
        
        total_assets = get_usd_balance(assets_node)
        total_liabilities = get_usd_balance(liabilities_node)
        net_worth = total_assets + total_liabilities
        
        bank_accounts = get_leaf_balances(assets_node.get('Bank'), strip_prefix="Assets:Bank:")
        retirement = get_leaf_balances(assets_node.get('Retirement'), strip_prefix="Assets:Retirement:")
        investments = get_leaf_balances(assets_node.get('Investments'), strip_prefix="Assets:Investments:")
        credit_cards = get_leaf_balances(liabilities_node.get('CreditCard'), strip_prefix="Liabilities:CreditCard:")
        
        housing = get_usd_balance(assets_node.get('Housing')) if assets_node.get('Housing') else 0.0
        cash = get_usd_balance(assets_node.get('Cash')) if assets_node.get('Cash') else 0.0

        return {
            "net_worth": round(net_worth, 2),
            "total_assets": round(total_assets, 2),
            "total_liabilities": round(total_liabilities, 2),
            "bank_accounts": bank_accounts,
            "retirement": retirement,
            "investments": investments,
            "credit_cards": credit_cards,
            "housing": housing,
            "cash": cash
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/api/ledger/pending")
async def get_pending_ledger():
    pending = []
    try:
        with open(LEDGER_PATH, "r", encoding="utf-8") as f:
            lines = f.readlines()
            for i, line in enumerate(lines):
                if line.startswith("20") and " ! " in line:
                    parts = line.split(" ! ")
                    date_str = parts[0].strip()
                    payee = parts[1].strip().strip('"')
                    
                    if i + 1 < len(lines):
                        next_line = lines[i+1].strip()
                        cat_parts = next_line.split()
                        if len(cat_parts) >= 3:
                            category = cat_parts[0]
                            amount = float(cat_parts[1])
                            pending.append({
                                "date": date_str,
                                "payee": payee,
                                "amount": amount,
                                "category": category,
                                "line_num": i
                            })
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
    return pending

@app.get("/api/ledger/categories")
async def get_ledger_categories():
    categories = []
    try:
        with open(LEDGER_PATH, "r", encoding="utf-8") as f:
            for line in f:
                if line.startswith("20") and " open Expenses:" in line:
                    parts = line.split(" open ")
                    if len(parts) > 1:
                        cat = parts[1].split()[0]
                        categories.append(cat)
        # Deduplicate and sort
        return sorted(list(set(categories)))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

@app.post("/api/ledger/approve")
async def approve_ledger(req: LedgerApproveRequest):
    try:
        with open(LEDGER_PATH, "r", encoding="utf-8") as f:
            lines = f.readlines()

        if req.line_num >= len(lines):
            raise HTTPException(status_code=400, detail="Invalid line number")

        # 1. Clear the exclamation mark on the transaction line
        tx_line = lines[req.line_num]
        lines[req.line_num] = tx_line.replace(" ! ", " * ", 1)

        # 2. Update the category on the next line
        cat_line = lines[req.line_num + 1]
        parts = cat_line.split()
        if len(parts) >= 3:
            old_category = parts[0]
            lines[req.line_num + 1] = cat_line.replace(old_category, req.category, 1)

        # Write back
        with open(LEDGER_PATH, "w", encoding="utf-8") as f:
            f.writelines(lines)

        return {"status": "success"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

if __name__ == "__main__":
    import uvicorn
    # Using port 8080 to avoid clashing with Navis backend on 8000
    uvicorn.run(app, host="0.0.0.0", port=8080)
