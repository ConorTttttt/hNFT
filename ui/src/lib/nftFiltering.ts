import type { AgentPubKey } from "@holochain/client";
import type { NftView } from "./appState.svelte";
import { b64, eqHash, nftTitle } from "./format";

/**
 * Pure NFT-grid filtering/search for the active collection's Items view. Only
 * filters/fields whose truth this application can already determine from an
 * already-loaded `NftView` (i.e. from `app.discover`, sourced from
 * `getAllNfts()`) — no fabricated market data, no network request per query,
 * and no cross-collection scan. Nothing here changes generative/blind-box
 * behavior, which stays wherever it already lives (`GenerativeStatusPanel`,
 * `BlindBoxesView`).
 *
 * Searchable fields (see the Phase 3 audit in the implementation report):
 *   - title: `nftTitle(manifest)` — the revealed "name" attribute, or the
 *     existing "Sealed NFT"/"Untitled" placeholders. Already present on every
 *     loaded `NftView`, so searching it costs nothing extra.
 *   - id: `b64(nft)` — the same base64 rendering `NftDetail`'s "Copy NFT ID"
 *     already uses. Already present on every loaded `NftView`.
 * Nothing else is searched: no generation seed/randomness/proof, no blind-box
 * secret, no owner key, and no private/unrevealed metadata — none of that is
 * present in `NftView` in the first place.
 */
export type NftFilter = "all" | "mine" | "unrevealed" | "revealed";

export const NFT_FILTER_LABEL: Record<NftFilter, string> = {
  all: "All",
  mine: "My NFTs",
  unrevealed: "Unrevealed",
  revealed: "Revealed",
};

export function filterNfts(views: NftView[], filter: NftFilter, me: AgentPubKey): NftView[] {
  switch (filter) {
    case "all":
      return views;
    case "mine":
      return views.filter((v) => eqHash(v.owner, me));
    case "unrevealed":
      return views.filter((v) => v.manifest.payload.status === "sealed");
    case "revealed":
      return views.filter((v) => v.manifest.payload.status === "revealed");
  }
}

/** `true` if `view`'s title or id contains `query` (case-insensitive, substring). */
export function nftMatchesQuery(view: NftView, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    nftTitle(view.manifest).toLowerCase().includes(q) || b64(view.nft).toLowerCase().includes(q)
  );
}

/** Local, in-memory title/id search over already-loaded views. Not a network
 * query, not global, not owner search — see this module's own doc comment. */
export function searchNfts(views: NftView[], query: string): NftView[] {
  const q = query.trim();
  if (!q) return views;
  return views.filter((v) => nftMatchesQuery(v, q));
}

// ---------------------------------------------------------------------------
// Sorting (Phase 5)
//
// AUDIT CONCLUSION — read before adding a sort mode here.
//
// Mint/creation order: NOT reliably available. `NftView` carries only
// `latest` (from `get_nft`, which walks the reveal chain), so its action
// timestamp is the MINT time for a still-sealed NFT but the REVEAL time for a
// revealed one — two different, silently conflated meanings. `app.discover`'s
// array order is `get_all_nfts`'s link-fetch order (`query.rs`), which
// Holochain does not guarantee to be chronological. Neither is a safe basis
// for "Newest"/"Oldest": NOT IMPLEMENTED. (Fix would mean exposing the
// original mint record's own timestamp on `NftView` — a client-state change,
// not a protocol change, but out of scope for this pass; see the phase
// report.)
//
// Price: `ListingView.price` is already a plain number (no unsafe string
// parsing needed), but `currency` is an arbitrary free-text field a seller
// types per listing (`ListingSection.svelte`) — nothing in the protocol
// normalizes currency per collection, so two listings can legitimately use
// different currencies. Comparing their `price` numbers directly would
// silently treat unrelated currencies as economically equivalent, which is
// explicitly unsafe. NOT IMPLEMENTED.
//
// Listed-first: reliable. "Listed" is exactly the same boolean the existing
// "For sale only" filter already uses (membership in the shared
// `AppState.listingByNft` map, itself `allListings()` loaded once). NOT a new
// query, and it composes losslessly with sort or filters.
// ---------------------------------------------------------------------------

export type NftSort = "default" | "listed-first";

export const NFT_SORT_LABEL: Record<NftSort, string> = {
  default: "Default order",
  "listed-first": "Listed first",
};

/**
 * Pure reordering only — never changes which views are present. `listedIds`
 * is the set of b64 NFT hashes with an active listing (build it from the
 * already-loaded `AppState.listingByNft`'s keys; no new query). The sort is
 * stable: relative order within the "listed" and "unlisted" groups is
 * preserved from the input.
 */
export function sortNfts(
  views: NftView[],
  sort: NftSort,
  listedIds: ReadonlySet<string>,
): NftView[] {
  if (sort === "default") return views;
  const listed: NftView[] = [];
  const unlisted: NftView[] = [];
  for (const v of views) {
    (listedIds.has(b64(v.nft)) ? listed : unlisted).push(v);
  }
  return [...listed, ...unlisted];
}
