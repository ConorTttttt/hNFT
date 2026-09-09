# @p2p-art-marketplace/ui

Svelte 5 + TypeScript + Vite front end. All Holochain/domain logic lives in
[`../client`](../client) (framework-agnostic); this package only does connection,
reactivity, persistence, and rendering.

```
npm install        # also builds ../client via its `prepare` script
npm run check      # svelte-check
npm run dev        # vite dev server (needs a conductor — see below)
npm run build      # type-check + production build to dist/
```

## Running against a conductor

- **Dev** — launch with `hc-spin` from the repo root; it packs the hApp, starts a
  conductor, injects the port + token, and opens this UI. `connectHolochain()`
  then works with no config.
- **Manual dev server** — `npm run dev` alone has no conductor to talk to. Point
  it at one by setting `VITE_HC_ADMIN_PORT` (and optionally `VITE_HC_PORT`,
  `VITE_HC_APP_ID`); the app then mints its own single-use app-auth token via the
  admin interface.

## Architecture

| Path | Responsibility |
|---|---|
| `src/lib/holochain.ts` | open the `AppWebsocket` (dev vs. packaged token flow) |
| `src/lib/appState.svelte.ts` | `AppState` — Svelte 5 runes stores (`myNfts`, `inbox`, `detail`, `collection`, loading/error) + signal wiring; the only place components call the client |
| `src/lib/saltStore.ts` | IndexedDB persistence of reveal commitments, keyed by NFT mint hash — **losing this makes a sealed NFT unrevealable** |
| `src/lib/context.ts` | put/get `AppState` in Svelte context |
| `src/lib/format.ts` | hash ⇄ base64, countdown formatting |
| `src/components/*` | Mint form, wallet grid, NFT card, NFT detail, reveal flow, transfer dialog + countdown, offer inbox, provenance timeline |

### Reactivity & signals

`AppState` subscribes once (`NftClient.onSignal`). `TransferOffered` refreshes the
inbox; `TransferAccepted` refreshes the wallet + open detail; local post-commit
signals (`EntryCreated` / `LinkCreated` / …) refresh everything. Refreshes are
debounced so a burst of signals collapses to one reload — components don't
re-fetch manually after actions.

### Reveal salt

`MintForm` calls `prepareCommitment` (generates the salt), mints, then
`AppState.mint` persists `{ salt, attributes, imageUri }` to IndexedDB under the
new NFT's hash. `RevealFlow` reads it back. A card / detail view shows
"salt missing" when the commitment isn't on this device, and reveal is blocked.
