# 🧭 Navis-Recon

> **High-speed financial reconnaissance and ledger reconciliation hub for the Sovereign master ledger.**

[![Project Board](https://img.shields.io/badge/GitHub-Project_%236-00d4ff?logo=github&logoColor=white)](https://github.com/users/KevinHozak/projects/6)
[![Frontend](https://img.shields.io/badge/Frontend-Vite_8_+_React_19-646CFF?logo=vite&logoColor=white)](frontend/)
[![Backend](https://img.shields.io/badge/Backend-FastAPI-009688?logo=fastapi&logoColor=white)](backend/)
[![Ledger](https://img.shields.io/badge/Engine-Beancount_AST-red)](backend/main.py)
[![Styling](https://img.shields.io/badge/Styling-Tailwind_CSS_v4-38BDF8?logo=tailwindcss&logoColor=white)](frontend/src/index.css)

---

## 🏛️ Overview

**Navis-Recon** is the dedicated financial command center for the Hozak household. Designed as a focused, high-precision interface for `Sovereign.beancount`, it balances a traditional respect for generational stewardship with a forward-thinking, dark-mode fintech terminal.

Unlike bloated third-party SaaS budgeting tools or complex gaming-themed lore portals, **Navis-Recon** is completely standalone, private, and deterministic. It operates directly against plain-text double-entry accounting records, prioritizing speed, data integrity, and tactile transaction approval.

---

## ✨ Key Features

* **⚡ High-Speed Triage & Approval:** Instantly scans `Sovereign.beancount` for pending (`!`) transactions, surfaces AI/rule-guessed categories, and executes atomic line-level approvals (`*`) back to disk.
* **🏦 The Vault (Wealth Dashboard):** Parses the Beancount ledger into an Abstract Syntax Tree (AST) using `beancount.loader` to generate real-time net worth visualizations, balance curves, and category drill-downs via dynamic Recharts.
* **📋 Sovereign Statement Checklist:** Audits monthly statements against a structured registry to guarantee zero unverified bank activity.
* **🔄 Live Sync Pipeline:** Automated financial harvesting and statement sync engine with real-time Server-Sent Events (SSE) streaming progress to the UI.
* **📱 Progressive Web App (PWA):** Installable, responsive web application tuned for desktop command and mobile review with offline shell caching.

---

## 📐 Architecture & Technology Stack

```
Navis-Recon/
├── backend/
│   ├── main.py              # FastAPI application & Beancount loader API (Port 8080)
│   └── requirements.txt     # Python dependencies
├── frontend/
│   ├── src/                 # React 19 SPA (Port 5173)
│   │   ├── components/      # LedgerReviewWidget, WealthDashboardWidget, SyncWidget, etc.
│   │   └── App.jsx          # Primary shell & view router
│   ├── index.html           # PWA entrypoint
│   └── vite.config.js       # Vite 8 & PWA configuration
├── docs/
│   └── architecture.md      # Detailed system blueprints & vault mandates
├── start_all.vbs            # Detached dual-service launcher
└── Stop_Recon.bat           # Port 8080 / 5173 process termination script
```

### Stack Components
* **Frontend:** [React 19](https://react.dev/), [Vite 8](https://vite.dev/), [Tailwind CSS v4](https://tailwindcss.com/), [Lucide Icons](https://lucide.dev/), [Recharts](https://recharts.org/), [Framer Motion](https://www.framer.com/motion/).
* **Backend:** [FastAPI](https://fastapi.tiangolo.com/), [Uvicorn](https://www.uvicorn.org/), Python 3.14.
* **Accounting Engine:** [Beancount](https://beancount.github.io/) (Pure Python AST parser).
* **Data Manifold:** Sourced securely from `G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount`.

---

## 🚀 Getting Started

### Prerequisites
* **Python 3.11+** with `beancount`, `fastapi`, and `uvicorn` installed.
* **Node.js 20+** and `npm`.
* Local access to the Navis-Vault ledger directory.

### Quick Start (Windows)
To launch both the FastAPI backend and Vite frontend silently in the background:
```cmd
start_all.vbs
```

To stop all running services:
```cmd
Stop_Recon.bat
```

### Manual Development

#### Backend
```bash
cd backend
python main.py
# API runs at http://localhost:8080 (Swagger UI at http://localhost:8080/docs)
```

#### Frontend
```bash
cd frontend
npm install
npm run dev
# Interface opens at http://localhost:5173
```

---

## 🛡️ Core Directives & Integrity

1. **Beancount Integrity:** Never manipulate account balances with loose regex math. All mutations are performed via targeted string swaps (e.g., flipping `!` to `*`) to protect comments, formatting, and file structure.
2. **Standalone Boundary:** Navis-Recon does not route traffic through or share state with Navis-Web. It remains an independent, highly resilient utility.
3. **Strict Chronology & Vault Protocols:** All incoming financial artifacts adhere to canonical filing standards (`[Institution] [YYYY-MM-DD].pdf`) inside `Navis-Vault/Ledger/`.

---

## 📊 Project Tracking & Tauri Migration Roadmap

Development tasks and phased migration epics are tracked on the official **[Navis-Recon GitHub Project Board (#6)](https://github.com/users/KevinHozak/projects/6)**.

### 🦀 Tauri v2 Desktop Migration (DreamCatcher Architecture)
- [ ] **Phase 1: Shell & IPC Scaffold** ([#1](https://github.com/KevinHozak/Navis-Recon/issues/1)) — Initialize `src-tauri` shell, window styling, and React IPC abstraction.
- [ ] **Phase 2: Checklist & Triage Engine** ([#2](https://github.com/KevinHozak/Navis-Recon/issues/2)) — Port `checklist.json` operations directly to Rust.
- [ ] **Phase 3: Ledger Review & Approvals** ([#3](https://github.com/KevinHozak/Navis-Recon/issues/3)) — Port pending ledger scans and targeted approvals to Rust.
- [ ] **Phase 4: Beancount Parser & Telemetry Engine** ([#4](https://github.com/KevinHozak/Navis-Recon/issues/4)) — Native Rust Beancount AST and balance realization for wealth metrics.
- [ ] **Phase 5: Financial Harvester & Event Streaming** ([#5](https://github.com/KevinHozak/Navis-Recon/issues/5)) — Bridge Python statement harvester via Tauri event channels.
- [ ] **Phase 6: Standalone Executable Packaging & Sunset** ([#6](https://github.com/KevinHozak/Navis-Recon/issues/6)) — Bundle `Navis-Recon.exe` and deprecate the FastAPI background server.