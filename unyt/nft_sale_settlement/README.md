# NFT sale settlement

Settles an NFT sale in Unyt: the buyer's parked spends pay the seller exactly
the agreed price, and the RAVE records which NFT was paid for.

## Inputs

| Input | Source | Meaning |
|---|---|---|
| `buyer_spends` | `ProvidedBy: buyer_spender` (aggregate) | The buyer's parked spend links, each `{ amount, source }` |
| `seller` | `Fixed` on the agreement | AgentPubKey paid for the NFT |
| `price` | `Fixed` on the agreement | Unit map; only index `"0"` is priced |
| `nft_id` | `Fixed` on the agreement | Id of the NFT being sold |

## Output

- `unyt_allocation`: one allocation paying `price` to `seller`, sourced from every buyer spend.
- `computed_values`: `{ nft_id, price }` for the NFT transfer to check against.

## Roles

- `buyer_spender` (`ParkedSpendBalance`): the buyer. Any agent may fill it.

## Trust assumptions

- **Price and seller are fixed on the agreement**, not chosen by the executor.
  Under `executor_rules: Any`, anyone can execute it, but nothing they can
  supply changes who is paid or how much.
- **Exact match or refusal.** The script throws unless the buyer's spends total
  exactly `price`. Consumed inputs are burned even when nothing is paid, so
  refusing before allocating is what protects the buyer's spend.
- **One-time run** (`one_time_run: true`). A sale settles once.
- **No lock, no carry.** Nothing is held back, so `executor_rules: Any` is
  safe here. Any future version that adds `locked` must switch to
  `AuthorizedExecutor` (see the skill's `output_contract.md`).
- **Only unit `"0"` is priced.** Other unit indexes in a spend are ignored,
  not refunded. A buyer paying in another unit gets a refusal, since their
  base-unit total won't match.

## Not in this template

- Delivering the NFT. The RAVE records the payment; the NFT transfer is a
  separate step that must check this RAVE's `computed_values` against the
  NFT, and must verify the RAVE itself rather than trusting the caller's read.
- Partial payment, overpayment, refunds, and escrow hold. Those need a
  different template.
