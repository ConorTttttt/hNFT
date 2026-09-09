# Demoing the app

## 1. One-time setup

Electron's sandbox helper needs to be setuid-root (Ubuntu blocks the unprivileged
sandbox, and disabling it breaks hc-spin's conductor injection):

```bash
sudo chown root:root ~/hNFT/node_modules/electron/dist/chrome-sandbox
sudo chmod 4755      ~/hNFT/node_modules/electron/dist/chrome-sandbox
```

Redo this only if you reinstall `node_modules/electron`. `scripts/live.sh` checks
it and prints these commands if it's not set.

## 2. Start

From a **normal terminal** (not `nix develop` — the script pulls the Holochain
binaries out of the nix store itself):

```bash
cd ~/hNFT
npm start                 # == bash scripts/live.sh
#   AGENTS=3 npm start     # 3 windows instead of 2
```

Builds the hApp if needed → starts Vite on `:1420` → `hc-spin` opens **N Electron
windows** (agent **A**, agent **B**, …), each loading the UI, all wired to one
fresh conductor.

`Ctrl-C` in that terminal stops everything. ~1–1.5 GB RAM.

## 3. Orient

Each window's header:

- **left** — `Dev Collection` · `DEV` pill · `minter` pill (dev mode: everyone
  can mint).
- **centre** — `Collection` · `Discover` (disabled — needs a `get_all_nfts`
  coordinator fn) · `Activity` (shows a count badge when offers are waiting).
- **right** — `● N peers` · ☀/☾ theme toggle · your **identicon badge** (click to
  copy your agent key) · **`+ Mint`**.

The point of the demo: you **never click Refresh**. Cards, the inbox, the detail
view, and the activity log all update themselves from Holochain signals.

## 4. Mint → reveal → transfer → accept

### Mint — window A

1. **`+ Mint`** (header) → modal.
2. **Image URI** — use something that renders after reveal:
   `https://picsum.photos/seed/bloom/512`, or a `data:` URI, or `ipfs://demo`
   (that one will show a placeholder — browsers can't load `ipfs://`).
3. Attributes — the `name` row becomes the card title (e.g. `name` / `Cosmic
   Bloom`). Add more rows with `+ attribute`.
4. **Mint sealed NFT**. Toast: *"Minted — publishing to peers…"*.
5. **Collection** — a card appears on its own, titled, tagged **sealed**, with
   your identicon. No refresh: the local `EntryCreated` signal drove it.

### Reveal — window A

1. Click the card → detail dialog (`minter: you`, `owner: you`).
2. **Reveal metadata** (uses the salt saved in this window's IndexedDB at mint).
3. Status flips to **revealed**; the image + attributes render. The card behind
   updates too.

### Transfer — window A → B

1. In window B, click the **identicon badge** in the header to copy B's key.
2. Window A: open the NFT → **Transfer** → paste B's key → **Send offer**. The
   dialog auto-closes; toast *"Offer sent to …"*.

### Accept — window B

1. A toast appears in B: **"New transfer offer — open it"**. Click it →
   B jumps to the **Activity** tab and the offer row **flashes**.
   (Or just click the Activity tab — the badge shows `1`.)
2. The offer shows a thumbnail, `from <A>`, `hop #1`, and a live **countdown**
   (from 5:00 — `transfer_window_secs` in `dna.yaml`).
3. **Accept**.

### After the sale

- **Window B → Collection** — the NFT is here (revealed image), no refresh.
- **Window A** — the card leaves A's collection on its own (the wallet
  self-heals: it reconciles against `current_owner` and drops the stale index
  link). Open the NFT from anywhere and **Provenance** shows `#1  you → <B>`;
  `owner` is B.

## 5. Worth showing

| Try | What it demonstrates |
|---|---|
| Transfer a **sealed** NFT (skip reveal), then **A** reveals it from the **"Awaiting your reveal"** shelf at the top of Collection | The minter can still reveal after transferring on — the zome gates reveal on *being the minter*, not ownership. B (the new owner) can't reveal — no salt, not the minter. |
| Let an offer's countdown hit 0:00 in B before accepting | **Accept** disappears (not just disabled); an `expired` pill + **Hide** replace it, and the row drops into the collapsible **Resolved & hidden** section. After the window, A can also **cancel** it. |
| B transfers onward to A (or a 3rd window with `AGENTS=3`) | Provenance grows to `#2`; `current_owner` follows the chain; a past owner can't re-offer. |
| Toggle ☀/☾ | Full light theme; the choice persists (localStorage). |
| Hover a card → **⊗**, or detail → **Hide from collection** | Per-device hide; hidden cards live in the collapsible **Hidden · N** section with an unhide control. Never touches on-chain state. |
| **Hide** an offer, then expand **Resolved & hidden** → **Unhide** | Same reversible per-device pattern for offers; accepted/expired/hidden are visually distinct. |
| Watch the header `● N peers` | Passive DHT-sync indicator (polled), instead of a refresh button. |
| Reload a window (`Ctrl-R`) | State rebuilds from the DHT + that window's IndexedDB salts. Connection has a 15s timeout + **Retry** button if the conductor is slow. |

## Troubleshooting

- **"Could not connect"** — Vite wasn't up when hc-spin loaded it; `Ctrl-R` the
  window (or click **Retry**).
- **`kitsune2-bootstrap-srv ENOENT` / missing binary** — `live.sh` didn't get the
  full holonix PATH; re-run `npm start` from a fresh terminal.
- **Window won't close / frozen modal dialog** — from another terminal:
  `pkill -9 -f 'hc-spin|electron/dist/electron|vite'`.
- **`WARNING: … globally installed electron … v42.x`** — harmless; it uses the
  bundled v42.
