# Echo Desktop: Security, Hygiene & System Boundary Guidelines

> **Mandatory Policy:** This document establishes non-negotiable boundaries for all automated agents, subagents, and developers working on the Echo Desktop project.

---

## 1. Absolute Local System Protection Contract

1. **Zero System File Modification:**
   * Under **no circumstances** may any agent, process, or script modify, delete, or overwrite files located in:
     - `C:\Windows`, `C:\Windows\System32`, `C:\Program Files`, `C:\Program Files (x86)` (outside read-only inspection of existing tools).
     - `%USERPROFILE%` (`C:\Users\<user>\`), `%APPDATA%`, `%LOCALAPPDATA%`.
     - System hosts files (`C:\Windows\System32\drivers\etc\hosts`), DNS settings, or network adapters.
   * Modifying global Windows Registry keys (`HKLM`, `HKCU\Environment`, `HKCU\Software`) is **strictly forbidden**.

2. **D: Drive Sandbox Guarantee:**
   * All toolchains, binaries, caches, package registry indices, build targets, and runtime scratch spaces must live strictly within **`D:\Echo Music`**:
     - Cargo Home: `D:\Echo Music\.cargo`
     - Rustup Home: `D:\Echo Music\.rustup`
     - Target Output: `D:\Echo Music\target`
     - Portable Tooling: `D:\Echo Music\.tools`
     - Node / Frontend: `D:\Echo Music\node_modules`

3. **No System Pollution on Exit:**
   * Environment variables (`RUSTUP_HOME`, `CARGO_HOME`, `PATH`) are scoped strictly to individual execution processes or local session scripts (`env.ps1`). They must **never** be permanently written to the global Windows system environment.

---

## 2. Secret Scanning & Pre-Commit Hygiene

Before any commit or remote push is authorized, the `security_auditor` subagent must verify:

1. **Zero Hardcoded Secrets:**
   * No API keys (Genius API tokens, YouTube API keys, Google credentials).
   * No private keys (`.pem`, `.key`, `.p8`, `.pkcs12`).
   * No personal access tokens (`gho_*`, `ghp_*`, `glpat-*`).
   * No connection strings, passwords, or cloud credentials (`AWS_*`, `AZURE_*`, `FIREBASE_*`).

2. **Sanitized Configuration Files:**
   * Configuration files must use placeholder schemas or read dynamically from `.env` files that are strictly listed in `.gitignore`.
   * Test fixtures must only use synthetic mock tokens.

---

## 3. Two-Stage Review & Audit Gate Before Remote Push

```
[ Code Change / Feature Implementation ]
                   │
                   ▼
       [ Automated Test Verification ]
                   │
                   ▼
       [ Security Auditor Subagent ]
         - Secret & Token Regex Scan
         - System Boundary Verification
                   │
                   ▼
       [ Code Reviewer Subagent ]
         - Specification Compliance
         - Memory Budget Contract (<65MB)
                   │
                   ▼
       [ GitHub Manager Subagent ]
         - Verified Remote Push (gh CLI)
```

No push shall ever occur without unanimous passing audits from both the **Security Auditor** and the **Code Reviewer**.
