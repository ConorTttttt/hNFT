# Unyt Smart Agreement templates

This directory holds Smart Agreement code templates authored for this app —
Rhai execution code plus the JSON schemas a Unyt `rave_engine` network needs
to run them. They're original work following the patterns in
[unytco/smart_agreement_library](https://github.com/unytco/smart_agreement_library)
(GPL-3.0), not copies of that library's own templates — a read-only reference
clone of that repo lives at `reference/unyt/` (gitignored, not part of this
build).

- **`nft_sale_escrow/`** — pays a seller the moment a buyer's locked payment
  exactly matches the seller's listed price for one NFT; the payment leg for
  `dnas/nft/zomes/coordinator/nft/src/payment.rs`'s
  `initiate_transfer_from_rave`. See its own `README.md` for what it does and
  does not do — in particular, it does not touch NFT ownership at all, and
  makes no atomicity guarantee with the transfer that reads its RAVE.

## This is not deployed anywhere by being in this repo

Placing a template file here makes it available to *build* and *version* — it
does not register it with any live Unyt network. Getting a template live on
the network this app is meant to run against (uploading it to that network's
template-library loader, whatever that mechanism is for your deployment) is a
separate step, outside what this repo can do on its own.
