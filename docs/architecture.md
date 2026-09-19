# Architecture: Navis-Recon

Navis-Recon was spun out of the Navis ecosystem to serve as a focused, lightweight financial hub. This document explains the core technical flow.

## The Data Source
The ultimate source of truth is a plain text file written in Beancount syntax:
`G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount`

## The Backend (FastAPI)
Located in `backend/main.py`, the backend runs on port 8080 to avoid clashing with Navis-Web (which runs on 8000).

It exposes three primary endpoints:
1. `GET /api/ledger/pending`
   - Scans `Sovereign.beancount` line by line for any transactions marked with the ` ! ` flag.
   - Returns a list of JSON objects containing the date, payee, amount, suggested category, and the exact line number in the file.
2. `GET /api/ledger/categories`
   - Scans `Sovereign.beancount` for `open Expenses:` directives.
   - Returns a sorted list of all valid expense accounts. Used to populate the Autocomplete Category Selector on the frontend.
3. `POST /api/ledger/approve`
   - Receives a line number and an approved category.
   - Replaces the ` ! ` flag with a ` * ` flag on the transaction line.
   - Replaces the old AI-guessed category with the user-approved category on the subsequent line.
   - Saves the file back to disk.
4. `GET /api/ledger/summary`
   - Uses `beancount.loader.load_file()` to parse the entire ledger into an AST (Abstract Syntax Tree).
   - Computes the exact real-time balances of the `Assets` and `Liabilities` trees.
   - Returns the Net Worth and top-level account breakdowns.

## The Frontend (Vite + React)
Located in `frontend/`, this is a React SPA running on port 5173. 

- **App.jsx**: The main layout. Maintains a state variable `currentView` to toggle between the Dashboard and the Review queue.
- **OverviewWidget.jsx**: The Dashboard. Fetches from `/api/ledger/summary` and visualizes the data.
- **LedgerReviewWidget.jsx**: The Triage Queue. Fetches from `/api/ledger/pending`. Maps over pending transactions and provides an interface to approve them.
- **CategorySelector.jsx**: A custom-built dropdown input that fuzzy-searches the available categories fetched from the backend.

## Operational Mandates (Vault Protocol)
Navis-Recon adheres to the structural and operational mandates established in the original Vault Protocol:

1. **Strict Chronology:** Statements must be archived using the format `[Institution] [YYYY-MM-DD].pdf`.
2. **Directory Structure:** No financial PDFs are allowed in the root. Everything must be filed within the `G:\My Drive\Navis-Vault\Ledger\` root directory:
   - `Statements/`: Archive of all income/expense reports.
   - `Policies/`: Insurance, Taxes, and long-term contracts.
   - `Sovereign.beancount`: Active tracking sheets and reconciliation reports.
3. **Audit First:** Every monthly reconcile requires a cross-reference between the **Master Checklist** and the bank activity.
4. **Security:** Never store plaintext passwords. Use Bitwarden Integration for all financial credentials.
