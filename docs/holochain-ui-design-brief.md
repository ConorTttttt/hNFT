# Holochain App UI/UX Design Brief

A house style for agent-centric, peer-to-peer apps. It is intentionally
**app-agnostic**: a marketplace, a chat, a scheduler and a document tool built
against it should feel like one family. Distilled from the P2P Art Marketplace
build; nothing here is NFT-specific.

Ship alongside this brief:

- **`holochain-ui-kit.css`** — the token layer + component primitives. Drop it in
  as your global stylesheet; it is the source of truth for anything below marked
  _(kit)_.
- **`identicon.ts`** — dependency-free agent avatars (§5.1).

---

## 1. Principles

1. **Dark-first, light-equal.** Design in dark, define light as a full token
   swap, respect `prefers-color-scheme`, make the toggle sticky. Neither mode is
   an afterthought.
2. **The agent is a person, not a hash.** Every public key gets an identicon and
   a short, copyable label. Raw 39-byte hashes never lead.
3. **State is pushed, not pulled.** The DHT tells you when things change. Wire
   signals to reactive stores; a "Refresh" button is an admission of failure.
   Keep one as an escape hatch, never as the primary path.
4. **Be honest about eventual consistency.** No fake progress bars for atomic
   calls. Say "published locally — propagating to peers." Say "waiting for the
   recipient to come online." Distinguish _local success_ from _network settled_.
5. **View preferences are local and reversible.** Hiding, dismissing, pinning,
   ordering — `localStorage`, per device, always a way back. Never mutate
   on-chain state for a cosmetic choice.
6. **Calm surface, loud data.** Low-chroma chrome, one accent, generous
   negative space, so the app's actual content (art, messages, documents) is
   what carries color and attention.
7. **Every latency has a shape.** Skeleton while loading, optimistic where safe,
   countdown where time-bound, "syncing" where gossip is in flight.

---

## 2. Colour

### 2.1 Tokens _(kit)_

Semantic, not literal. Components only ever reference these.

| Token | Role | Dark | Light |
|---|---|---|---|
| `--bg` | app canvas | `#0d0d16` | `#f6f6fb` |
| `--bg-elev` | raised inputs, headers, offer rows | `#151522` | `#ffffff` |
| `--bg-glow` | the single ambient background gradient stop | `#1b1533` | `#ece7ff` |
| `--panel` | cards, modals, sidebars | `#151522` | `#ffffff` |
| `--panel-2` | nested surface, chip background, hover fill | `#1f1e30` | `#f1f1f8` |
| `--border` | 1px structural dividers, card outline | `#2a2940` | `#e4e3ef` |
| `--border-strong` | interactive outline, input border | `#3a3856` | `#d0cfe0` |
| `--text` | primary | `#ededf5` | `#1b1a2c` |
| `--text-dim` | secondary, labels | `#a3a2bd` | `#55546e` |
| `--text-faint` | metadata, placeholders, hashes | `#6f6e8c` | `#8b8aa3` |
| `--accent` | primary action, active nav, focus ring | `#8b7cff` | `#6f5cff` |
| `--accent-hover` | hovered primary | `#9f93ff` | `#5a45f0` |
| `--accent-contrast` | text on `--accent` | `#ffffff` | `#ffffff` |
| `--accent-2` | links, secondary badges, "info" | `#62d8ff` | `#0aa6d0` |
| `--ok` | success, peer-online, settled | `#48d597` | `#1a9a6c` |
| `--warn` | pending, expiring soon | `#ffb800` | `#b57e00` |
| `--danger` | error, expired, destructive | `#ff5c5c` | `#e5484d` |
| `--ok-bg` / `--danger-bg` | 12% tints behind success/error blocks | — | — |

Elevation, radius, motion:

| Token | Value |
|---|---|
| `--radius` | `12px` (panels, cards, dialogs) |
| `--radius-sm` | `8px` (buttons, inputs, inline rows) |
| pill radius | `999px` |
| `--shadow` | `0 12px 40px rgba(0,0,0,.4)` dark · `…rgba(20,18,40,.12)` light |
| transitions | `120ms ease` for hover/opacity/colour; `150–160ms` for enter |

### 2.2 Semantic pairings

- **Status → colour is fixed across the whole product line:**
  `--ok` settled/accepted/online · `--warn` pending/expiring · `--danger`
  expired/failed/offline · `--accent-2` informational.
- **`--accent` is reserved for _the_ primary action** on a screen and the active
  nav item. If everything is accent, nothing is.
- **Never hardcode a hex in a component.** If you need a colour that isn't a
  token, you need a new token.

### 2.3 Theming mechanism _(kit)_

```
:root                    → dark tokens + color-scheme: dark
:root[data-theme=light]  → light tokens + color-scheme: light
```

- A `theme` module: read `localStorage`, else `matchMedia('(prefers-color-scheme: light)')`.
- Apply `document.documentElement.dataset.theme` **before mount** (no flash).
- One toggle in the header (`☀` / `☾`), persists on change.
- The ambient background gradient uses `--bg-glow` so it follows the theme; don't
  bake a colour into `body`.

### 2.4 Per-app accent

The **only** value a new app should routinely change is the accent pair. Keep
`--accent-2`, status colours, and all neutrals identical — that's what makes the
family read as a family. Pick an accent with enough contrast on both `--bg` and
`#ffffff`; the light-mode `--accent` is usually 1 step deeper than dark.

Examples for a product line: violet `#8b7cff` (default), teal `#4fd1c5`, amber
`#f0a95c`, rose `#f2789f`, indigo `#7c8cff`.

---

## 3. Typography

- **Inter Variable** (`@fontsource-variable/inter`), bundled, not CDN — Holochain
  apps must work offline.
- Stack: `"Inter Variable", "Inter", ui-sans-serif, system-ui, sans-serif`.
- **Monospace** for any hash, key, id, or countdown:
  `ui-monospace, "SF Mono", "JetBrains Mono", Menlo, monospace` at `0.85em`.
- Scale: body `14px/1.55`. Headings via weight + `letter-spacing: -0.01em`, not
  size jumps — `h3` ≈ 1rem/600, `h4` ≈ 0.95rem/600. Small print `0.82rem`.
- Weights: 400 body, 500 nav/labels, 600 headings & primary buttons. Avoid 700+.

---

## 4. Components _(kit)_

All of these are plain classes in `holochain-ui-kit.css`; no component library.

### 4.1 Buttons

| Class | Use |
|---|---|
| `.primary` | the one main action per view / dialog |
| _(default)_ | secondary actions |
| `.secondary` | secondary action you want tinted with `--accent-2` (e.g. "explore") |
| `.ghost` | tertiary / dismiss / icon buttons — transparent until hover |
| `.tiny` | dense rows, card corners |
| `.danger` | destructive, outlined in `--danger` |

Rules: exactly one `.primary` visible per context. Disabled = `opacity .4` +
`not-allowed`, never hidden mid-flow (except the expired-action case, §5.7).

### 4.2 Pills & chips

- **`.pill`** — uppercase 0.72rem status token. Variants tint themselves:
  `.pill.ok` / `.pill.warn` / `.pill.danger` / `.pill.accent` / `.pill.cyan`.
  Use for entry status, "expired", "accepted", counts.
- **`.chip`** — a rounded, bordered, _clickable_ affordance. The house pattern
  for "copy this value": chip = mono short value + a `⧉` glyph, click copies the
  full thing and toasts "copied".
- **`.dot`** — 7px status dot with a soft ring; green by default. `.peer.syncing
  .dot` pulses amber.

### 4.3 Panels & cards

- **`.panel`** — the workhorse container: `--panel` bg, 1px `--border`,
  `--radius`, `1rem` padding. Group everything in panels.
- **`.card`** — interactive tile: hover lifts `translateY(-2px)` + `--shadow` +
  `--border-strong`. A card that opens something is a `role="button"
  tabindex="0"` **div** (so it can nest a real corner button), with an
  `onkeydown` for Enter/Space.
- Media inside a card: 1:1 `.thumb`; when the image is missing or fails, swap to
  `.thumb.placeholder` (centered, faint, uppercase label) — never a broken image
  icon.

### 4.4 Fields

- **`.field`** — column: a `--text-dim` `0.82rem` label above the control.
- Inputs: `--bg-elev` fill, `--border-strong` outline, focus = 2px
  `--accent` outline at 55% via `color-mix`.
- Full-width by default; use `.row` + `.grow` for inline groups.

### 4.5 Dialogs

- Native `<dialog>` + `showModal()`. Kit styles the chrome + a blurred
  `::backdrop`.
- Open in an `$effect` on mount; the parent renders `{#if open}<Dialog/>{/if}`
  and clears its flag in the dialog's `onclose`.
- **Auto-close on success** for "fire and forget" dialogs (send offer, invite,
  etc.) ~1.2s after confirmation, so modals don't stack across repeated actions.
- Header row: title (+ status pill under it) left, `Close ✕` ghost right.

### 4.6 Toasts

- Bottom-right stack. `.toast.info` / `.success` / `.error` (3px left border).
- Auto-dismiss: 4s info/success, 7s error, **8s if actionable**.
- **Actionable toasts**: the body is a link-styled button reading
  `"<message> →"`; clicking runs the action _and_ dismisses. Inbound-signal
  toasts ("New offer", "You were invited") should be actionable and **navigate +
  highlight** the relevant item (switch view, scroll into view, flash the row
  for ~3s with an accent ring).
- Toasts report outcomes; they don't ask questions. Unwrap Holochain
  `Guest("…")` errors to the bare message before showing.

### 4.7 Empty states

- **`.empty`** — one-line dashed placeholder for a sub-section ("Nothing waiting
  for you.").
- **`.empty-hero`** — full-width onboarding for a primary view: bold line +
  muted explainer + **dual action** (`.primary` = create the first thing,
  `.secondary` = go discover). Distinguish "genuinely empty" from "everything is
  filtered/hidden" with different copy.

### 4.8 Loading

- **`.skeleton`** — shimmering block at the shape of the thing that's coming
  (card aspect ratio, row height). Use for first load of a list.
- Never a bare spinner for list content. A spinner is acceptable only for a
  single in-flight action button ("Minting…").

### 4.9 Segmented nav

- **`.nav`** — pill-shaped segmented control, `--panel-2` track, active item
  filled `--accent`. 2–4 items max. This is top-level IA, not tabs-within-a-page.
- **`.count`** badge on a nav item = _actionable_ items only (pending, unread).
  Accepted/expired/hidden things must not inflate it.
- Features that need a coordinator function you haven't written yet: show the
  nav item **disabled** with a `title` explaining why, so the IA is visible and
  honest.

---

## 5. Holochain-specific patterns

### 5.1 Agent identity

- **Identicon** — deterministic SVG from the pubkey bytes (mirrored 5×5 blockie,
  hue from a byte). No dependency, no image hosting, instant peer recognition.
  Use `identicon.ts`.
- **`.agent` badge** _(kit)_ — identicon + mono label, one control. Label =
  `"you"` for self, else `shortHash(key, 6)` → `abc123…wxyz`. The whole badge
  copies the full base64 key on click.
- **Never** render a full agent key or entry hash as running text. If the full
  value matters (debugging, sharing), put it behind a chip/tooltip.
- Real usernames need a profiles zome — until then, identicon + short key is the
  identity. Optionally let a user assign local-only nicknames (`localStorage`,
  keyed by key, per §1.5).

### 5.2 Hash display

`shortHash(hash, head = 6)` → `head` chars + `…` + last 4, monospace. Always
paired with a copy affordance (chip, or a badge). Base64 (`encodeHashToBase64`),
never hex.

### 5.3 Peer / DHT sync indicator

- Passive, in the header: `● N peers` (green dot) from
  `AppWebsocket.dumpNetworkStats().transport_stats.connections.length`, polled
  every ~6s.
- `0` peers or an errored poll → `◌ syncing…` with the amber pulsing dot.
- This **replaces** the manual "Refresh" button as the ambient signal that the
  app is live.

### 5.4 Signal-driven reactivity

- One subscription (`client.on("signal")` / your client wrapper's `onSignal`),
  dispatched in a single state object.
- Map signal → **debounced** store reload (~150ms) so a gossip burst collapses to
  one fetch.
- Local `post_commit` signals only reach the acting agent. If another agent
  needs to know (a reveal, a status change on something they hold), the zome
  must `send_remote_signal` to them; the UI treats it the same as a local one.
- Toast on meaningful inbound signals; make those toasts navigate (§4.6).
- A session **Activity feed** (newest-first, relative timestamps via a
  `relativeTime()` helper, `.activity-item` rows) is the persistent record of
  "what just happened" — separate from transient toasts.

### 5.5 Honest async feedback

- An atomic zome call is **one** step. On return, say "done locally — publishing
  to peers", not a fabricated `Signing → Publishing → Done` sequence.
- When an action depends on another agent being online (direct transfer,
  countersigning), say so: _"Waiting for the recipient to come online to
  finalise."_
- Distinguish **local success** (your source chain) from **network settled**
  (validated + gossiped). Optimistic UI is fine for the former; don't claim the
  latter until you can observe it.

### 5.6 Provenance & history

- Render chains of custody / edit history as a **connected vertical timeline**
  (`.timeline` with a left rule and node dots): `#seq  <AgentBadge> → <AgentBadge>`.
- Ordered by the on-chain sequence, not by fetch order. Surface a gap/fork as an
  explicit error state rather than guessing.

### 5.7 Time-bounded flows (windows, sessions)

- Show a **countdown pill** (`mm:ss`), `--warn` under 60s, `--danger` at 0.
- When a window closes, the primary action (**Accept**, **Sign**) is **removed**,
  not just disabled — replaced by an `expired` pill and a neutral archive/hide.
- Two-phase handshakes: keep an **Active** list (actionable now) and a
  collapsible **Resolved & hidden** section (accepted / expired / dismissed),
  each row carrying its own status pill.

### 5.8 Per-device preferences

- Hide / dismiss / pin / reorder → a `localStorage` string-set keyed by
  hash. Provide `loadStringSet` / `persistStringSet` helpers and reuse them for
  every such feature.
- The hidden items go into a **collapsible section with an Unhide control** —
  never a one-way trapdoor.
- Mirror the flag onto your view models on every reload so components filter on
  a plain boolean.

---

## 6. Layout & IA

- **Sticky header**, `--bg-elev` at 88% + `backdrop-filter: blur`, 1px bottom
  border. Three zones:
  - **left** — status dot, app name, a `.pill` or two for context (collection
    symbol, "minter", role).
  - **centre** — `.nav` segmented control (2–4 top-level views).
  - **right** — peer indicator, theme toggle, `.agent` badge (self), **one
    `.primary`** call to action (`+ New`, `+ Mint`, `+ Compose`).
- **Workspace**: `max-width: 1180px`, centred, `1.5rem` padding.
- **Two-pane vs. tab**: if a secondary stream (inbox, activity) is glanceable and
  short, a right rail is fine; if it grows or has sub-states, make it a nav view
  with a `.count` badge. Don't do both.
- **Modals for focused create/act flows** (mint, transfer, invite, compose);
  **views for browse/manage**.
- Responsive: collapse the workspace to one column and let the header wrap under
  ~820px. Everything is flex/grid with relative units already.

---

## 7. Accessibility baseline

- All interactive elements reachable by keyboard; card-divs get `role="button"
  tabindex="0"` + Enter/Space handlers.
- Focus visible: 2px `--accent` outline (kit sets it on inputs; keep it on
  buttons).
- Colour is never the only signal — pair status colour with a word (`expired`,
  `accepted`) or icon.
- `prefers-color-scheme` honoured; `prefers-reduced-motion` should disable the
  shimmer and flash animations (add a media query if you add more motion).
- Identicons are `aria-hidden`; the adjacent text label is the accessible name.

---

## 8. Adopting the kit in a new app

1. Copy `holochain-ui-kit.css` → import once, globally (before your app styles).
2. Copy `identicon.ts` + your framework's `Identicon` / `AgentBadge` wrappers.
3. Set the app's `--accent` / `--accent-hover` pair (both themes). Change nothing
   else in `:root`.
4. Bundle Inter (`@fontsource-variable/inter`), import in your entry file.
5. Wire the theme module (read → apply before mount → toggle in header).
6. Build the header per §6. Even a one-view app gets the header (name, peer
   indicator, theme, identity, primary action).
7. Every list view: skeleton, `.empty` / `.empty-hero`, signal-driven reload, no
   manual refresh as the primary path.
8. Every agent reference: `AgentBadge`. Every hash: `shortHash` + copy.
9. Time-bounded or multi-party flows: §5.5–5.7.

---

## 9. Anti-patterns

- ❌ Raw agent keys / entry hashes as body text.
- ❌ A "Refresh" button as the main way to see new state.
- ❌ Fabricated multi-step progress for a single zome call.
- ❌ `.primary` on more than one action in view.
- ❌ Hardcoded hex colours in components; off-token greys.
- ❌ Hiding/dismissing something with no way to get it back.
- ❌ An active **Accept**/**Sign** button on an expired item.
- ❌ Light mode as a media-query-only afterthought with no toggle, or dark-only.
- ❌ CDN fonts / remote images with no offline fallback.
- ❌ Spinners for list loads; broken-image glyphs for missing media.
- ❌ Claiming "done" / "synced" before the network has actually settled.

---

## 10. Reference implementation

The P2P Art Marketplace `ui/` is the worked example:

| Pattern | Where |
|---|---|
| token layer + themes | `ui/src/app.css` |
| identicon / agent badge | `ui/src/lib/identicon.ts`, `ui/src/components/{Identicon,AgentBadge}.svelte` |
| theme module | `ui/src/lib/theme.svelte.ts` |
| signal → debounced store, activity log, peer poll | `ui/src/lib/appState.svelte.ts` |
| actionable toast that navigates + flashes | `ui/src/lib/toast.svelte.ts`, `components/{Toaster,OfferInbox}.svelte` |
| per-device hide/dismiss + unhide section | `AppState.{hideNft,dismissOffer,…}`, `components/{WalletGrid,OfferInbox}.svelte` |
| countdown pill / active vs resolved | `components/{Countdown,OfferInbox}.svelte` |
| provenance timeline | `components/ProvenanceTimeline.svelte` |
| honest async + local-vs-network | `AppState.mint`, `RemoteSignal::NftRevealed` |
