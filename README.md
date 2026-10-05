# 🧭 Navis-Recon

> **High-speed financial reconnaissance and ledger reconciliation hub for the Sovereign master ledger.**

[![Project Board](https://img.shields.io/badge/GitHub-Project_%236-00d4ff?logo=github&logoColor=white)](https://github.com/users/KevinHozak/projects/6)
[![Frontend](https://img.shields.io/badge/Frontend-Vite_8_+_React_19-646CFF?logo=vite&logoColor=white)](frontend/)
[![Desktop](https://img.shields.io/badge/Desktop-Tauri_v2-24C8DB?logo=tauri&logoColor=white)](frontend/src-tauri/)
[![Ledger](https://img.shields.io/badge/Engine-Native_Rust-red)](frontend/src-tauri/src/)
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
* **🔄 Live Sync Pipeline:** Automated financial harvesting and statement sync engine with real-time Tauri event streaming progress to the UI.
* **📱 Progressive Web App (PWA):** Installable, responsive web application tuned for desktop command and mobile review with offline shell caching.

---

## 📐 Architecture & Technology Stack

```
Navis-Recon/
├── frontend/
│   ├── src/                 # React 19 SPA
│   │   ├── components/      # LedgerReviewWidget, WealthDashboardWidget, SyncWidget, etc.
│   │   ├── services/        # apiClient.js (Tauri IPC invoke & event streaming)
│   │   └── App.jsx          # Primary shell & view router
│   ├── src-tauri/           # Native Rust Tauri v2 core
│   │   ├── src/
│   │   │   ├── checklist.rs     # Direct atomic checklist.json I/O
│   │   │   ├── ledger_parser.rs # Native Beancount parser & balance realized curves
│   │   │   ├── ledger_review.rs # Transaction scanning & atomic approval mutations
│   │   │   ├── sync.rs          # Harvester subprocess bridge & live event bus
│   │   │   └── lib.rs           # Tauri app handler & IPC command registry
│   │   ├── tauri.conf.json  # Window configuration & desktop permissions
│   │   └── Cargo.toml       # Rust dependencies
│   ├── index.html           # PWA entrypoint
│   └── vite.config.js       # Vite 8 & PWA configuration
├── docs/
│   ├── architecture.md      # Detailed system blueprints & IPC specifications
│   └── archive/             # Archived FastAPI server & legacy run scripts
├── scripts/                 # Desktop deployment & packaging automation
├── start.bat                # Launch packaged Navis-Recon.exe desktop app
├── stop.bat                 # Terminate running Navis-Recon processes
└── Navis-Recon.exe          # Standalone release desktop executable
```

### Stack Components
* **Desktop Shell:** [Tauri v2](https://v2.tauri.app/) (Windows native desktop window, IPC bridge, event streaming).
* **Accounting Engine:** Native Rust (`ledger_parser.rs`, `ledger_review.rs`) with atomic file replacements.
* **Frontend:** [React 19](https://react.dev/), [Vite 8](https://vite.dev/), [Tailwind CSS v4](https://tailwindcss.com/), [Lucide Icons](https://lucide.dev/), [Recharts](https://recharts.org/), [Framer Motion](https://www.framer.com/motion/).
* **Data Manifold:** Sourced securely from `G:\My Drive\Navis-Vault\Ledger\Sovereign.beancount` and `checklist.json`.
* **Harvester Bridge:** Python financial artifact classifier (`finance_processor.py`) orchestrated directly via Tauri child processes without a web server.

---

## 🚀 Getting Started

### Prerequisites
* **Rust 1.78+** and Cargo (`rustup default stable`).
* **Node.js 20+** and `npm`.
* Local access to the Navis-Vault ledger directory.

### Quick Start (Desktop)
To launch the compiled standalone desktop application:
```cmd
start.bat
```

To close or terminate running instances:
```cmd
stop.bat
```

### Development Workflow

#### 1. Desktop Development Mode (Vite + Tauri Dev)
Runs the desktop window with hot reloading for both React frontend and Rust backend:
```bash
cd frontend
npm install
npm run desktop
```

#### 2. Building the Standalone Desktop Executable
Compiles the optimized Rust binary, packages the desktop bundle, and places `Navis-Recon.exe` directly in the repository root:
```bash
cd frontend
npm run desktop:build
```

#### 3. Frontend-Only Development (Vite Dev Server)
If inspecting UI styling or components in a browser:
```bash
cd frontend
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

### 🦀 [Epic: Tauri Desktop Migration (#7)](https://github.com/KevinHozak/Navis-Recon/issues/7)
- [ ] **Tauri-P1: Shell & IPC Scaffold** ([#1](https://github.com/KevinHozak/Navis-Recon/issues/1)) — Foundational shell & IPC bridge. *(Blocks: #2, #3, #4, #5, #6)*
- [ ] **Tauri-P2: Checklist & Triage Engine** ([#2](https://github.com/KevinHozak/Navis-Recon/issues/2)) — Direct Rust `checklist.json` I/O. *(Depends on: #1 | Blocks: #6)*
- [ ] **Tauri-P3: Ledger Review & Approvals** ([#3](https://github.com/KevinHozak/Navis-Recon/issues/3)) — Rust pending line scans & approvals. *(Depends on: #1 | Blocks: #4, #6)*
- [ ] **Tauri-P4: Beancount Parser & Telemetry Engine** ([#4](https://github.com/KevinHozak/Navis-Recon/issues/4)) — Rust AST & balance realization. *(Depends on: #1, #3 | Blocks: #6)*
- [ ] **Tauri-P5: Financial Harvester & Event Streaming** ([#5](https://github.com/KevinHozak/Navis-Recon/issues/5)) — Tauri event streaming for harvester. *(Depends on: #1 | Blocks: #6)*
- [ ] **Tauri-P6: Standalone Executable Packaging & Sunset** ([#6](https://github.com/KevinHozak/Navis-Recon/issues/6)) — Bundle `Navis-Recon.exe` & retire FastAPI. *(Depends on: #1, #2, #3, #4, #5)*

## Standalone Desktop Architecture

Navis-Recon is packaged as a Tauri v2 Windows desktop application. The native Rust layer owns ledger parsing, checklist and review persistence, and the financial harvester bridge. The packaged executable does not require FastAPI, Uvicorn, or a frontend server on ports 8080 and 5173.

Build and launch the release application with:

```cmd
cd frontend
npm run desktop:build
cd ..
start.bat
```

The build also publishes `Navis-Recon.exe`, `Navis-Recon-Setup.exe`, and `Navis-Recon.lnk` directly in the repository root for convenient launching.

The custom compass-ledger application icon is authored in `frontend/src-tauri/icon-source.svg` and compiled into the Windows asset at `frontend/src-tauri/icons/icon.ico`. Retired FastAPI and dual-server launch artifacts are preserved under `docs/archive/`.
