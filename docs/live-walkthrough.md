# Live walkthrough — mint → reveal → transfer → accept

## Start it

From a **normal terminal** on the desktop (not `nix develop` — the script pulls the
Holochain binaries out of the nix store itself):

```bash
cd ~/hNFT
npm start          # == bash scripts/live.sh
```

What happens: builds the hApp if needed → starts the Vite dev server on `:1420` →
waits for it → `hc-spin -n 2` opens **two Electron windows**, agent **A** and
agent **B**, both loading the UI, wired to one fresh 2-agent conductor.

Stop everything with `Ctrl-C` in that terminal.

RAM: ~1–1.5 GB while running (brief ~2 GB spike on the first zome call).

---

## The click-through

Each window's header shows `Dev Collection (DEV)` and its own agent key
(`uhCAk…`, top-right, monospace). **Copy B's key** — you paste it into A later.

The whole point is that you **never click Refresh**. Cards, the inbox, and the
detail view update themselves from Holochain signals.

### 1. Mint — in window A

1. **Mint** tab.
2. Image URI: `ipfs://demo` (any string works).
3. Attribute row: `background` / `cobalt`. (`+ attribute` for more.)
4. **Mint**. → "Minted `uhCkk…` — sealed."
5. **Wallet** tab — a card appears on its own, tagged **sealed**, no "salt missing"
   badge (the salt was written to this window's IndexedDB).

*Signal:* the card showed up because the local `EntryCreated` post-commit signal
triggered a wallet reload.

### 2. Reveal — in window A

1. Click the sealed card → detail dialog. `minter: you`, `owner: you`.
2. **Reveal metadata**.
3. Dialog flips to **revealed** — the image and `background: cobalt` render.
4. Close it. The wallet card behind is already revealed too — no refresh.

### 3. Transfer — window A → B

1. Reopen the NFT's detail in A → **Transfer**.
2. Paste **window B's agent key** into the recipient field.
3. **Send offer** → "Offer sent…". Close the dialogs.

### 4. Accept — in window B

1. Look at B's **Inbox** (right sidebar) — the offer is already there. You did
   not refresh B; it arrived via the `TransferOffered` remote signal.
2. It shows `from <A short hash> · hop #1` and a live **countdown** ticking down
   from 5:00 (`transfer_window_secs = 300` in `dna.yaml`).
3. **Accept** (enabled while the countdown is > 0).

### 5. After the sale

- **Window B → Wallet**: the NFT is now here (revealed image), no refresh.
- **Window A → open the NFT detail**:
  - `owner:` is now B's short hash, not "you".
  - **Provenance**: `#1  you → <B hash>`.
  - A **"Remove stale wallet entry"** button appears — A still holds the derived
    `AgentToNft` index link even though B owns the NFT. Click it; the card leaves
    A's wallet. (This is the cache-vs-ledger split the `drop_stale_ownership_link`
    edge test covers — provenance was already authoritative.)

---

## Things worth trying

| Try | Expected |
|---|---|
| Transfer a **sealed** NFT to B, then B opens its detail | B sees "the reveal salt … is not stored on this device" — B never had the salt, so B can't reveal. Intentional. |
| Let an offer's countdown hit 0:00 in B's inbox before accepting | **Accept** disables, tag flips to **expired**. |
| B transfers the NFT onward to A (or a 3rd `-n 3` window) | Provenance grows to `#2`, `current_owner` follows the chain. |
| Reload an Electron window (Ctrl-R) | State rebuilds from the DHT + this browser's IndexedDB salts. |

## If a window shows "Could not connect"

The Vite server wasn't up when hc-spin loaded it. `Ctrl-R` in that window. The
`wait-on` guard in `live.sh` normally prevents this.
