## Security Audit Report: Solana MEV Live Trading Control Center

**Auditor:** GLM-5, Senior Web & Security Auditor
**Target:** Live Real-Money Trading Module (`server.js`, `index.html`)
**Classification:** **CRITICAL / HIGH RISK**

### Executive Summary
The proposed module transitions the dashboard from a passive monitoring tool to an **active financial control system**. While the feature set is comprehensive, the current architectural design presents **multiple critical attack vectors** that could lead to immediate financial loss. A breach in this module does not just leak data; it drains wallets.

The following audit identifies critical flaws in Authentication, Input Validation, Command Injection, and Key Management.

---

### 1. Critical Security Findings

#### A. Insecure Direct Object Reference (IDOR) & Lack of Authorization
**Severity: CRITICAL**
The requirements list `POST /api/live/control` and `POST /api/live/config` but make no mention of authentication middleware.
*   **Vulnerability:** If the dashboard is exposed to the public internet or accessed via a local network without strict firewall rules, any user (or scanner) can trigger the `Emergency Killswitch` or, worse, modify trading parameters to siphon funds (e.g., setting `min spread` to 0 and `max position size` to maximum).
*   **Recommendation:**
    *   Implement strict Session-based authentication or Mutual TLS (mTLS) for all `/api/live/*` endpoints.
    *   Implement Role-Based Access Control (RBAC). A "Viewer" role should not be able to access `POST` endpoints.
    *   Add a CSRF token implementation for all state-changing requests.

#### B. Operating System Command Injection
**Severity: CRITICAL**
The requirement to query PM2 status and control the engine implies executing shell commands on the host system.
*   **Vulnerability:** If `server.js` constructs the command string using unsanitized inputs (e.g., `exec(`pm2 ${userInput}`)`), an attacker can inject arbitrary shell commands (e.g., `; rm -rf /`).
*   **Recommendation:**
    *   **Never** pass user input directly to shell execution functions (`exec`, `spawn`).
    *   Use the `pm2` Node.js API programmatic module (`pm2.connect`, `pm2.stop`) rather than CLI wrappers. This removes the shell interpreter from the execution chain.
    *   Whitelist allowed actions: `const ALLOWED_ACTIONS = ['start', 'stop', 'restart']; if (!ALLOWED_ACTIONS.includes(action)) return 403;`

#### C. Private Key Exposure & Wallet Security
**Severity: CRITICAL**
*   **Vulnerability:** The requirement "verifies connected wallet public address" is ambiguous. If the frontend ever asks for a **Private Key** or **Seed Phrase** to "verify" or "sign" via the dashboard, user funds are at immediate risk.
*   **Recommendation:**
    *   **Strict Public-Key Only:** The dashboard UI must **only** accept Public Keys.
    *   **Backend Wallet Verification:** The backend should verify the wallet by querying the RPC node (`getBalance`). Do not import keypairs into the dashboard server environment unless absolutely necessary for signing (which is an anti-pattern).
    *   **Key Storage:** If the live engine requires a keypair, it must be stored in an encrypted Environment Variable or a Hardware Security Module (HSM), never in `config.json` or the database.

#### D. Denial of Service (DoS) via RPC Loop
**Severity: HIGH**
*   **Vulnerability:** The endpoint `POST /api/live/wallet` queries the Mainnet RPC. If the dashboard UI polls this endpoint aggressively (e.g., every 1 second) or if an attacker spams this endpoint, you will hit RPC rate limits, causing the dashboard to lock up and potentially miss critical trading signals.
*   **Recommendation:**
    *   Implement server-side caching for wallet balances (e.g., cache for 5-10 seconds).
    *   Rate limit API endpoints using `express-rate-limit`.

---

### 2. Logic & UX Flaws

#### A. Race Conditions in Control Logic
*   **Issue:** Sending a "Start" command via API while the engine is already initializing could corrupt the state or spawn duplicate processes.
*   **Recommendation:** Implement a state machine on the backend. The API should return `409 Conflict` if the engine is transitioning states (e.g., `STARTING` or `STOPPING`).

#### B. "Emergency Killswitch" Reliability
*   **Issue:** A web request is an unreliable way to stop a high-frequency trading bot. If the server hangs or the network disconnects, the killswitch signal never arrives.
*   **Recommendation:** Implement a "Dead Man's Switch" pattern. The bot should stop automatically if it loses connection to the dashboard/heartbeat for $N$ seconds, rather than relying solely on an explicit stop signal.

#### C. Parameter Validation Gaps
*   **Issue:** `POST /api/live/config` allows updating `max position size`.
*   **Risk:** Setting this value higher than the available wallet balance or below the exchange minimum will cause the bot to crash or waste SOL on failed transaction fees.
*   **Recommendation:** Backend must validate that `max_position_size < (wallet_balance - reserve_floor)`.

---

### 3. Formal Recommendations

#### Implementation Checklist for `server.js`:

1.  **Middleware Stack:**
    ```javascript
    // Pseudo-code for security stack
    app.use('/api/live', rateLimiter({ windowMs: 15 * 60 * 1000, max: 100 })); // Limit requests
    app.use('/api/live', authMiddleware); // Verify JWT or Session
    app.post('/api/live/control', validateActionInput, handleControl);
    ```

2.  **Safe PM2 Control:**
    ```javascript
    // Use the programmatic API
    const pm2 = require('pm2');
    
    function controlEngine(action) {
        return new Promise((resolve, reject) => {
            pm2.connect((err) => {
                if (err) reject(err);
                // Whitelisted action mapping
                if (action === 'stop') {
                    pm2.stop('rust-mev-arb-live', (err) => { ... });
                }
                // ...
            });
        });
    }
    ```

3.  **NDJSON Parsing Safety:**
    *   `live_trades.ndjson` should be read using a streaming parser to prevent memory overflow if the file grows large. Do not `fs.readFileSync` and split by lines if the file exceeds 10MB.

#### Implementation Checklist for `index.html`:

1.  **Visual Feedback:**
    *   The "Emergency Killswitch" button must be physically separated from other buttons and require a confirmation modal (e.g., "Type 'KILL' to confirm").
    *   Use distinct color coding: Green (Running), Red (Stopped), Orange (Error/Unknown).

2.  **Wallet Handling:**
    *   Add a warning banner: *"This dashboard does not store private keys. Ensure you are interacting with the correct bot process."*

3.  **Data Freshness:**
    *   Display a "Last Updated" timestamp next to the wallet balance. If the RPC fails, show "RPC Error" clearly rather than 0 balance (which might trigger a false "Low Balance" alert).

### Conclusion
The architecture is functionally sound but **operationally dangerous** in its current state. You are building a remote control for a money-printing (or burning) machine.

**Verdict:** **Do Not Deploy** to production until Authentication and Command Injection vulnerabilities are patched. The "Emergency Killswitch" must be tested rigorously to ensure it functions under network degradation.