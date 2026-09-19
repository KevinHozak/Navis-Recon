# 🤖 AGENTS.md

> [!IMPORTANT]
> Read this for project startup, architecture mapping, and system-wide directives when pair-programming on Navis-Recon.

## 🏛️ Project Overview
**Navis-Recon** is a standalone Tauri desktop application designed as the primary financial hub for the Hozak family. It serves as a visual frontend and triage interface for the `Sovereign.beancount` master ledger.

Unlike Navis-Web, which is heavily themed around sci-fi RPG lore, Navis-Recon is a clean, hyper-focused financial dashboard that prioritizes data precision, net worth tracking, and high-speed ledger review.

---

## 🗂️ Workspace Architecture

The workspace contains a React frontend and a native Rust desktop backend:

1. **`frontend/` (Vite + React)**
   - Runs on `http://localhost:5173`.
   - Uses Tailwind CSS v4 for styling. 
   - Uses a sidebar routing layout to swap between the Overview Dashboard and Pending Reviews.
2. **`backend/` (FastAPI + Python)**
   - Runs on `http://localhost:8080`.
   - Uses the official `beancount` Python engine to parse the ledger.
   - Responsible for fetching pending transactions, extracting categories, and modifying the underlying `.beancount` file.
3. **`docs/` (Documentation)**
   - Houses the architectural blueprints.

---

## ⚙️ Environmental Context & Startup

### 1. Unified Run Scripts
- **Startup Script:** `start_all.vbs` in the root directory launches both the FastAPI backend and Vite frontend concurrently in detached, hidden processes.
- **Stop Script:** `Stop_Ledger.bat` (or `Stop_Recon.bat`) kills active instances on ports 8080 and 5173.

### 2. Vault Mapping
The backend requires access to the Obsidian vault to read the Beancount ledger:
- **Ledger Path:** `G:\My Drive\Navis-Protocol\Facilities\Vault\Ledger\Sovereign.beancount`

---

## 🎨 Styling & Design Aesthetics

Navis-Recon maintains a premium aesthetic. Any modifications must adhere to these rules:
1. **Premium Dashboard Look:** Use dark modes with clean borders (`border-white/10`) and sleek container backgrounds (`bg-zinc-950`).
2. **Color Palette:** 
   - **Accent:** `#00d4ff` (cyan) is used for active states, highlights, and primary assets.
   - **Warnings/Liabilities:** `red-500` for debts, liabilities, and pending notification badges.
3. **Animations:** Keep interactions snappy but smooth using Tailwind's `transition-all` and simple `animate-pulse` classes where appropriate.
4. **No Tailwind Placeholders:** Use Tailwind v4 native imports in `index.css`.

---

## ⚠️ Important Rules for AI Agents

1. **Beancount Integrity:** Never use raw string manipulation to change balances. The `beancount.loader` engine is strictly used for *reading* the ledger. When modifying lines for approvals, the backend performs highly specific targeted string replacements (e.g., swapping `!` for `*`) to preserve formatting and alignment.
2. **Decoupled from Navis-Web:** DO NOT attempt to route traffic, share state, or link back to the Navis-Web application. Navis-Recon is completely standalone.

## Standalone Desktop Packaging

The packaged application is a Tauri v2 Windows executable. Rust IPC owns ledger parsing, checklist and review persistence, and the harvester bridge. Use `start.bat` and `stop.bat` for desktop startup and shutdown; the archived FastAPI/server-era scripts under `docs/archive/` are not part of the runtime. The release build is `cd frontend && npm run desktop:build`, and it must not require Uvicorn or ports 8080/5173.
