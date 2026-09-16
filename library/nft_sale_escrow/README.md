# NFT Sale Escrow

Pays a seller the moment a buyer's locked payment exactly matches the seller's listed price for one
NFT. This is the **payment leg** of an NFT sale, built on the same parked-data / parked-spend matching
pattern as the library's `pos_offline_escrow` template.

## Read this first: what this does NOT do

Unyt Smart Agreements move fungible unit-map value (`unyt_allocation`, `locked`, `credit_limit`) between
agents. There is no NFT primitive anywhere in `rave_engine` or this library: no mint, no uniqueness
constraint, no ownership index, and no way for Rhai to call into another DNA's zome (the registered
helper catalog has nothing that touches a foreign cell). So this template:

- **Does not mint or hold the NFT.** `nft_id` is an opaque string it never validates — it's whatever
  identifier your NFT registry app uses (that app's own entry/action hash, typically).
- **Does not enforce that the seller actually owns the NFT**, or prevent the same `nft_id` from being
  listed in two agreement instances at once. That check belongs to the NFT registry app.
- **Does not transfer ownership.** It produces a RAVE recording `nft_id`, `previous_owner`, and
  `new_owner` (the buyer's key, resolved cryptographically — see below) — a companion app is expected to
  watch for that RAVE and act on it.

If you don't already have an NFT registry app, build one with a general Holochain skill (entries for
token metadata, links from an owner's agent key to their tokens, validation callbacks enforcing one
owner at a time). This template is the payment half; it is not a substitute for that app.

## Wiring this to an NFT registry

The realistic integration pattern, and its real limitation:

1. The seller lists by parking a `listing` (below) in this agreement instance, **and** separately marks
   the NFT "for sale" (e.g. locked/escrowed for transfer) in the registry app, naming this agreement's
   `ea_id` or instance so the registry can later correlate the RAVE.
2. The buyer parks their `escrow_lock`.
3. Settlement runs, paying the seller and emitting `computed_values.new_owner`.
4. A bridging process (the marketplace UI, or a scheduled/triggered call from the registry app's own
   coordinator zome) reads that RAVE and calls the registry's transfer function.

**Step 4 is not atomic with step 3.** These are two separate DNAs; there is no shared transaction across
them. A design that mitigates this: have the registry app require the NFT to already be reserved
(unlisted from general sale) before this agreement's escrow can settle, so the worst case is "payment
settled, transfer call pending/retried" rather than "double-sold." A design that does not mitigate it:
letting the registry's transfer depend on nothing but hoping the bridging process runs. Say this plainly
to anyone building on this template — don't imply the chain guarantees an atomic swap it can't.

## Participants

### `seller`

Parks a `ParkedData` link (consumed once read) with the listing: `nft_id`, `unyt_index`, `price`, and
their own payout `receiver` key. Role id deliberately avoids the send/collect substrings (`sender`,
`spender`, `receiver`, `payee`, ...) so the UI renders it as a data submission, not a spend or collect.

### `buyer_spender`

Parks a `ParkedSpendBalance` link locking exactly the listed price. The `spender` substring in the role
id is load-bearing for the UI. Swap `ParkedSpendBalance` for `ParkedSpendCredit` in both
`agreement_definition_input.json` and the agreement file if buyers should pay on credit — nothing else
in this template changes.

### Executor

Set on the live agreement, not a role. The example uses `{ "AuthorizedExecutor": seller_key }` so only
the seller's own device triggers settlement of their own listing — not strictly required by the engine
here (only a `locked` output forces `AuthorizedExecutor`), but it keeps an unrelated third party from
triggering someone else's sale.

### Open or restricted buyer

The example restricts `buyer_spender` to one `Authorized` key (a pre-agreed private sale). For an open
listing anyone can buy at the listed price, set that role's `qualification` to `{ "Any": null }` instead.
Nothing else in the template needs to change — `buyer_key` is still derived from whoever actually parked
the escrow lock, not from the role qualification.

## Inputs

### `listing` — `ProvidedBy: seller`

```json
{
  "nft_id": "uhCEk_registry_apps_own_token_hash",
  "unyt_index": "0",
  "price": "250",
  "receiver": "uhCAk_seller_agent_key"
}
```

### `escrow_lock` — `ProvidedBy: buyer_spender`

```json
{
  "amount": { "0": "250" },
  "source": "uhCkk_buyer_parked_spend_hash"
}
```

## Code Execution Flow

1. **Fail closed** if either the listing or the escrow lock hasn't been parked yet.
2. **Read the listing's `unyt_index` and `price`.**
3. **Confirm the lock covers that unit index at all.**
4. **Compare the locked amount to the price with an exact match**, both directions, via
   `is_greater_than_eq` — see Security Considerations for why exact match.
5. **Resolve the buyer's identity** from the escrow lock's parked-link creator via
   `get_spend_links_author`, rather than trusting a claim — this is the value a registry app should treat
   as authoritative for "who paid."
6. **Release the lock**: one `unyt_allocation` entry paying the seller's `receiver` the locked amount,
   citing the lock's own `source`.
7. **Report `computed_values`**: `nft_id`, `unyt_index`, `settled_amount`, `previous_owner`, `new_owner`,
   `fulfilled: true` — the record a registry app's bridging process reads.

## Output Structure

### `unyt_allocation`

- `receiver`: the seller's own declared payout key from the listing.
- `amounts`: a single-entry unit map at the listing's `unyt_index`, equal to the locked amount.
- `sources`: the buyer's escrow lock hash.

### `computed_values`

- `nft_id`, `unyt_index`, `settled_amount`, `previous_owner`, `new_owner`, `fulfilled`.

## Security Considerations

- **Exact-match assumption, stated explicitly**: the buyer's lock must equal the price precisely, not
  merely cover it. Partial draws and change-making (via the engine's `consume_allocations` helper) are
  out of scope for this template, for the same reason `pos_offline_escrow` excludes them: real complexity
  and a real risk of stranding a buyer's remainder if done wrong.
- **`new_owner` is derived, not claimed**: it comes from `get_spend_links_author` against the parked
  spend link Holochain already validated as signed by the buyer — not from an `ExecutorProvided` field,
  which would let the executor claim any buyer they liked.
- **Per-listing binding is structural, not hash-comparison**: nothing in the Rhai code compares `nft_id`
  against anything else. The binding between "this listing" and "this lock" comes from creating a fresh
  agreement instance per listing, the same way each sale should be its own instance. Reusing one instance
  across multiple unrelated listings is a misuse of this template.
- **One resolution per instance**: `one_time_run: true` — a listing that's already sold can't be resold
  by re-triggering the same instance.
- **This is not proof of NFT ownership.** Nothing here checks that the seller who parked `nft_id` is the
  registry app's recorded owner. If that matters (it almost always does), the registry app must be the
  one that gates listing creation, not this template.
