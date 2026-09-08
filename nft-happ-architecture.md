# NFT hApp Architecture — P2P Art Marketplace (reconciled against `countersigning.md`)

**Status:** revised design, ready to scaffold with one item to spike in Sweettest first (see §7).
**References used:** `architecture.md`, `patterns.md`, `progenitor.md`, `countersigning.md` from the
holochain skill. Toolchain: `hdk = "=0.7.0"`, `hdi = "=0.8.0"`, holonix `main-0.7`.

---

## 0. What changed after reading `countersigning.md`

The earlier plan assumed a countersigned bundle of `{TransferRecord create + DeleteLink + CreateLink}`.
The reference rules that out and pushes back on countersigning generally:

1. **One entry per session.** `PreflightRequest` carries a single `app_entry_hash` and a single
   `action_base: ActionBase`. A session commits *exactly one entry per agent* — never a bundle of
   entry + link ops. Any `AgentToNft` link maintenance is a separate, non-atomic write regardless.

2. **Stock conductors do not ship it.** Countersigning is behind `unstable-countersigning`, off by
   default. holonix and Kangaroo binaries do not have it compiled in. Choosing it means building and
   distributing a custom conductor — an architectural commitment, not a late feature flag.

3. **The reference's own first checklist item is the alternative:** *"Can the invariant be expressed
   as validation on two independent entries instead? That needs no feature flag and no chain freeze."*
   For NFT ownership transfer, it can.

4. **No `validate()`-side `Entry::CounterSign` accessor is documented.** The reference's position:
   the conductor's system validation verifies session legitimacy *before* `validate()` runs, so
   `validate()` should check semantic content only and not try to detect the countersign wrapper.

**Consequence:** this design leads with a **no-countersigning two-phase handshake** (Design A) and
keeps countersigning as a documented alternative (Design B) for teams already committed to a custom
conductor.

---

## 1. Data Model Strategy (Integrity Zome: `nft_integrity`)

### 1.1 DNA Properties — per-collection config (not an entry)

Per `progenitor.md`: put this in a shared `utils` crate, derive `SerializedBytes`, read via
`dna_info()?.modifiers.properties.try_into()`. Safe in `validate()` because it is part of the DNA
hash and therefore identical for every validator by construction (unlike `init_properties`, which is
conductor-local and invisible to validation).

```rust
// dnas/nft/zomes/coordinator/utils/src/dna_properties.rs
use hdi::prelude::*;

#[derive(Serialize, Deserialize, SerializedBytes, Clone, Debug)]
pub struct CollectionProperties {
    pub name: String,
    pub symbol: String,
    pub max_supply: u32,
    /// Base64 AgentPubKey string; the only agent permitted to mint in this collection.
    /// `None` = dev/bootstrap mode (no cap enforced).
    pub minter_pubkey: Option<String>,
    #[serde(default)]
    pub royalty_bps: u16, // 0 in v1; unenforced seam for v2
    /// Seconds a TransferCommitment stays open for the buyer to accept before the
    /// seller may cancel it. Baked into the DNA so every validator agrees.
    pub transfer_window_secs: u64,
}

impl CollectionProperties {
    pub fn get() -> ExternResult<Self> {
        dna_info()?.modifiers.properties.try_into().map_err(|e| {
            wasm_error!(WasmErrorInner::Guest(format!("bad DNA properties: {e}")))
        })
    }
    pub fn minter() -> ExternResult<Option<AgentPubKey>> {
        match Self::get()?.minter_pubkey {
            None => Ok(None),
            Some(s) => AgentPubKey::try_from(s)
                .map(Some)
                .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("bad minter key: {e}")))),
        }
    }
}
```

> **`minter_pubkey` reuses the progenitor primitive.** It is the collection's minting authority.
> Setting it at deploy time is the exact mechanism from `progenitor.md` §5 (dna.yaml / Sweettest
> `DnaModifiersOpt::with_properties` / Kangaroo `roles_settings`). Leave it `null` for local dev.

### 1.2 Entries

```rust
use hdi::prelude::*;

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct NftManifest {
    /// blake2b-256 commitment: hash(salt || canonical_bytes(attributes, image_uri)).
    pub metadata_hash: [u8; 32],
    pub payload: NftPayload, // Sealed at mint; Revealed via update_entry
    // NO token_id, NO owner, NO minter, NO minted_at.
    //   - identity is this entry's *original Create ActionHash*
    //   - minter / minted_at are already on the Create action header
    //   - ownership is derived from the transfer provenance chain (§1.4)
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum NftPayload {
    Sealed,
    Revealed {
        salt: [u8; 32],
        attributes: Vec<NftAttribute>,
        image_uri: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NftAttribute {
    pub trait_type: String,
    pub value: String,
}

/// Authored by the CURRENT OWNER. An open offer to hand `nft` to `to_agent`.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct TransferCommitment {
    pub nft: ActionHash,       // NftManifest original Create action = NFT identity
    pub to_agent: AgentPubKey,
    /// The ownership state this offer supersedes:
    ///   - `nft` itself for the first transfer (mint -> first owner move), or
    ///   - the ActionHash of the previously ACCEPTED TransferReceipt.
    pub prev_state: ActionHash,
    pub sequence: u32,         // 1 for the first transfer, increments per hop
}

/// Authored by the BUYER named in the commitment. Accepting the offer.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct TransferReceipt {
    pub commitment: ActionHash, // the TransferCommitment Create action being accepted
}

/// Authored by the SELLER. Voids an unaccepted commitment so the NFT-state can be
/// re-offered. Only valid once the transfer window has elapsed (see validation).
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct TransferCancellation {
    pub commitment: ActionHash,
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    NftManifest(NftManifest),
    TransferCommitment(TransferCommitment),
    TransferReceipt(TransferReceipt),
    TransferCancellation(TransferCancellation),
}
```

**Why `to_agent` / `sequence` are entry fields and not "already in the header":** a
`TransferCommitment` is authored solely by the seller, so the header records only the seller. The
counterparty and the hop number are not derivable from any single action header — they are genuine
entry content. `prev_state` is an embedded `ActionHash` (per `patterns.md` "Embedded ActionHash"):
it is intrinsic to this commitment's identity, and it never changes after commit, so no update chain
is needed.

### 1.3 Link Types

```rust
#[hdk_link_types]
pub enum LinkTypes {
    // Discovery
    PathToNft,            // "collection.all" path  -> NftManifest original action
    // Reveal chain (patterns.md Update Pattern)
    NftUpdates,           // NftManifest original action -> update action (Sealed -> Revealed)
    // Provenance (append-only, ordered by TransferReceipt.sequence via the commitment)
    NftToReceipt,         // NftManifest original action -> TransferReceipt action
    // Handshake routing (lets the buyer find offers addressed to them without a global scan)
    AgentToInboundCommitment, // buyer AgentPubKey -> TransferCommitment action
    // DERIVED ownership index (NOT source of truth) — fast "what does X own" lookup.
    // LinkTag = the authorizing TransferReceipt ActionHash (or the mint action for seq 0).
    AgentToNft,           // owner AgentPubKey -> NftManifest original action
}
```

**`AgentToNft` is a cache, not the ledger.** Ground truth for "who owns NFT N": walk `NftToReceipt`
from N, take the receipt whose chain of commitments reaches the highest `sequence`, and that
commitment's `to_agent`. `AgentToNft` exists only so `get_nfts_for_agent` is one `get_links` call
instead of a provenance walk per NFT.

### 1.4 Ownership, defined precisely

> NFT `N` is owned by agent `X` at a given moment iff the longest valid chain
> `mint -> Commitment_1 -> Receipt_1 -> Commitment_2 -> Receipt_2 -> ...` starting at `N` ends in a
> `TransferReceipt` whose referenced `TransferCommitment.to_agent == X`; or, if no receipt exists,
> iff `X` is the author of `N`'s mint action.

Every link in that chain is verified with `must_get_*` on immutable actions, so the definition is
deterministic and every validator computes the same owner.

### 1.5 Validation Rules

All rules below use only: fields of the op, `dna_info()`, `must_get_action`,
`must_get_valid_record`, `must_get_agent_activity`, and action-header timestamps. No `get`,
`get_links`, `agent_info`, or wall-clock `sys_time` — per `patterns.md` determinism rules.

#### `validate_create_nft_manifest` (mint)

```rust
pub fn validate_create_nft_manifest(
    action: TypedAction<EntryCreationData>,
    entry: NftManifest,
) -> ExternResult<ValidateCallbackResult> {
    let props = CollectionProperties::get()?;

    // 1. Minter authority (progenitor.md pattern). None = dev mode, no restriction.
    if let Some(minter_str) = props.minter_pubkey.as_ref() {
        let minter = AgentPubKey::try_from(minter_str.clone())
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e}"))))?;
        if action.author() != &minter {
            return Ok(ValidateCallbackResult::Invalid(
                "only the collection minter may mint".into(),
            ));
        }
    }

    // 2. Must mint Sealed.
    if !matches!(entry.payload, NftPayload::Sealed) {
        return Ok(ValidateCallbackResult::Invalid("NFT must mint Sealed".into()));
    }
    if entry.metadata_hash == [0u8; 32] {
        return Ok(ValidateCallbackResult::Invalid("metadata_hash must not be zero".into()));
    }

    // 3. max_supply — deterministic because the minter's own source chain is strictly
    //    ordered by construction. Walk it backward from this mint's predecessor and
    //    count prior NftManifest creates. `must_get_agent_activity` DEFERS on an
    //    incomplete chain (UnresolvedDependencies) instead of failing, so validators
    //    converge. A malicious minter forking their chain produces a warrant.
    if props.minter_pubkey.is_some() && props.max_supply > 0 {
        let prev = action
            .prev_action()
            .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("mint has no prev_action".into())))?
            .clone();
        let activity = must_get_agent_activity(
            action.author().clone(),
            ChainFilter::new(prev),
        )?;
        let minted_so_far = activity
            .iter()
            .filter(|a| is_nft_manifest_create(a.action.hashed.content.action()))
            .count() as u32;
        if minted_so_far >= props.max_supply {
            return Ok(ValidateCallbackResult::Invalid("collection max_supply reached".into()));
        }
    }

    Ok(ValidateCallbackResult::Valid)
}
```

> `is_nft_manifest_create` matches `Action::Create` whose `entry_type()` is the `AppEntryDef` for
> `UnitEntryTypes::NftManifest` in this zome. Exact ergonomics of reading entry type off a
> `RegisterAgentActivity` record — **verify against `hdi 0.8` at implementation** (see §7). The
> logic is sound; only the accessor spelling is unconfirmed by the reference.

#### `validate_update_nft_manifest` (reveal — commit-reveal check)

```rust
pub fn validate_update_nft_manifest(
    action: TypedAction<UpdateData>,
    entry: NftManifest,
    original_action: TypedAction<EntryCreationData>,
    original_entry: NftManifest,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid("only the minter may reveal".into()));
    }
    if !matches!(original_entry.payload, NftPayload::Sealed) {
        return Ok(ValidateCallbackResult::Invalid("already revealed".into()));
    }
    if entry.metadata_hash != original_entry.metadata_hash {
        return Ok(ValidateCallbackResult::Invalid("metadata_hash changed on reveal".into()));
    }
    let NftPayload::Revealed { salt, ref attributes, ref image_uri } = entry.payload else {
        return Ok(ValidateCallbackResult::Invalid("update must reveal, not stay Sealed".into()));
    };
    let mut preimage = salt.to_vec();
    preimage.extend(
        holochain_serialized_bytes::encode(&(attributes, image_uri))
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?,
    );
    if blake2b_256(&preimage) != entry.metadata_hash {
        return Ok(ValidateCallbackResult::Invalid(
            "revealed metadata does not match commitment".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
```

#### `validate_create_transfer_commitment` (seller opens an offer)

```rust
pub fn validate_create_transfer_commitment(
    action: TypedAction<EntryCreationData>,
    entry: TransferCommitment,
) -> ExternResult<ValidateCallbackResult> {
    let seller = action.author().clone();

    // 1. The seller must currently own `entry.nft`, established by `prev_state`:
    if entry.prev_state == entry.nft {
        // First transfer: prev_state points at the mint. Seller must be the minter/author.
        let mint = must_get_action(entry.nft.clone())?;
        if mint.action().author() != &seller {
            return Ok(ValidateCallbackResult::Invalid(
                "first transfer must be authored by the NFT's minter".into(),
            ));
        }
        if entry.sequence != 1 {
            return Ok(ValidateCallbackResult::Invalid("first transfer must have sequence 1".into()));
        }
    } else {
        // Later transfer: prev_state is the previously accepted TransferReceipt.
        let prev_receipt_rec = must_get_valid_record(entry.prev_state.clone())?;
        let prev_receipt: TransferReceipt = prev_receipt_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest("prev_state is not a TransferReceipt".into())))?;
        let prev_commitment_rec = must_get_valid_record(prev_receipt.commitment.clone())?;
        let prev_commitment: TransferCommitment = prev_commitment_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest("dangling commitment".into())))?;

        if prev_commitment.nft != entry.nft {
            return Ok(ValidateCallbackResult::Invalid("prev_state belongs to a different NFT".into()));
        }
        if prev_commitment.to_agent != seller {
            return Ok(ValidateCallbackResult::Invalid(
                "seller does not hold the prior accepted ownership state".into(),
            ));
        }
        if entry.sequence != prev_commitment.sequence + 1 {
            return Ok(ValidateCallbackResult::Invalid("sequence must increment by 1".into()));
        }
    }

    // 2. Uniqueness of the offered state on the SELLER's own chain. The seller may not
    //    have two live commitments superseding the same `prev_state`. Deterministic:
    //    the seller's chain is linear; walk it and reject a duplicate that lacks a
    //    matching (post-window) TransferCancellation authored by the same seller.
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("commitment has no prev_action".into())))?
        .clone();
    let activity = must_get_agent_activity(seller.clone(), ChainFilter::new(prev))?;
    // Pseudocode: for each earlier Create of TransferCommitment C' by seller with
    // C'.prev_state == entry.prev_state, require an earlier/interleaved Create of
    // TransferCancellation by seller with .commitment == hash(C'). If any such C'
    // has no cancellation -> Invalid("nft-state already offered").

    Ok(ValidateCallbackResult::Valid)
}
```

#### `validate_create_transfer_receipt` (buyer accepts)

```rust
pub fn validate_create_transfer_receipt(
    action: TypedAction<EntryCreationData>,
    entry: TransferReceipt,
) -> ExternResult<ValidateCallbackResult> {
    let commitment_rec = must_get_valid_record(entry.commitment.clone())?;
    let commitment: TransferCommitment = commitment_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest("commitment entry missing".into())))?;

    // 1. Only the named buyer may accept.
    if action.author() != &commitment.to_agent {
        return Ok(ValidateCallbackResult::Invalid("only the named buyer may accept".into()));
    }

    // 2. Accept-vs-cancel race resolved by the transfer window, using ACTION TIMESTAMPS
    //    (which ARE available in validate()). The window bound is baked into DNA props.
    let props = CollectionProperties::get()?;
    let opened_at = commitment_rec.action().timestamp();
    let accepted_at = action.timestamp();
    let deadline = (opened_at + core::time::Duration::from_secs(props.transfer_window_secs))
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?;
    if accepted_at > deadline {
        return Ok(ValidateCallbackResult::Invalid("acceptance after transfer window".into()));
    }

    // 3. Buyer must not have already accepted this commitment (buyer's own linear chain).
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("receipt has no prev_action".into())))?
        .clone();
    let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
    // Pseudocode: reject if an earlier Create of TransferReceipt by this agent already
    // references entry.commitment.

    Ok(ValidateCallbackResult::Valid)
}
```

#### `validate_create_transfer_cancellation` (seller voids an unaccepted offer)

```rust
pub fn validate_create_transfer_cancellation(
    action: TypedAction<EntryCreationData>,
    entry: TransferCancellation,
) -> ExternResult<ValidateCallbackResult> {
    let commitment_rec = must_get_valid_record(entry.commitment.clone())?;
    let commitment: TransferCommitment = commitment_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest("commitment entry missing".into())))?;

    // Only the original offerer may cancel, and only AFTER the window closes, so a
    // cancellation and a valid acceptance can never both exist for one commitment.
    if action.author() != commitment_rec.action().author() {
        return Ok(ValidateCallbackResult::Invalid("only the offerer may cancel".into()));
    }
    let props = CollectionProperties::get()?;
    let opened_at = commitment_rec.action().timestamp();
    let cancelled_at = action.timestamp();
    let deadline = (opened_at + core::time::Duration::from_secs(props.transfer_window_secs))
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?;
    if cancelled_at <= deadline {
        return Ok(ValidateCallbackResult::Invalid("cannot cancel before the window closes".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}
```

#### `validate_create_link_*`

```rust
// PathToNft: base is the collection path hash, target is an NftManifest Create action.
// Mint-authority is already enforced on the entry; here just confirm the target exists.
pub fn validate_create_link_path_to_nft(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = action.data.target_address.clone().into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest("bad target".into())))?;
    must_get_valid_record(target)?;
    Ok(ValidateCallbackResult::Valid)
}

// NftToReceipt: base is the NFT, target is a TransferReceipt whose commitment names that NFT,
// and the link author is a party to the transfer.
pub fn validate_create_link_nft_to_receipt(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let nft = action.data.base_address.clone().into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest("bad base".into())))?;
    let receipt_hash = action.data.target_address.clone().into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest("bad target".into())))?;
    let receipt_rec = must_get_valid_record(receipt_hash)?;
    let receipt: TransferReceipt = receipt_rec.entry().to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest("target is not a TransferReceipt".into())))?;
    let commitment_rec = must_get_valid_record(receipt.commitment.clone())?;
    let commitment: TransferCommitment = commitment_rec.entry().to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest("dangling commitment".into())))?;
    if commitment.nft != nft {
        return Ok(ValidateCallbackResult::Invalid("link base != commitment.nft".into()));
    }
    if action.author() != commitment_rec.action().author()
        && action.author() != &commitment.to_agent
    {
        return Ok(ValidateCallbackResult::Invalid("link author is not a party to the transfer".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

// AgentToNft: DERIVED index. LinkTag carries the authorizing evidence hash.
//   - mint case:   tag = the mint action hash; base_agent must be the mint's author.
//   - transfer:    tag = a TransferReceipt action hash; that receipt's commitment must
//                  have to_agent == base_agent and nft == link target. Deterministic:
//                  one must_get_valid_record on the exact hash in the tag, no traversal.
pub fn validate_create_link_agent_to_nft(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let base_agent = AgentPubKey::try_from(action.data.base_address.clone())
        .map_err(|_| wasm_error!(WasmErrorInner::Guest("bad base".into())))?;
    let nft = action.data.target_address.clone().into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest("bad target".into())))?;
    let evidence = ActionHash::try_from(action.data.tag.clone().into_inner())
        .map_err(|_| wasm_error!(WasmErrorInner::Guest("tag is not an ActionHash".into())))?;
    let rec = must_get_valid_record(evidence)?;

    match rec.entry().to_app_option::<TransferReceipt>()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
    {
        Some(receipt) => {
            let c_rec = must_get_valid_record(receipt.commitment.clone())?;
            let c: TransferCommitment = c_rec.entry().to_app_option()
                .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
                .ok_or(wasm_error!(WasmErrorInner::Guest("dangling commitment".into())))?;
            if c.nft == nft && c.to_agent == base_agent {
                Ok(ValidateCallbackResult::Valid)
            } else {
                Ok(ValidateCallbackResult::Invalid("AgentToNft evidence does not authorize this owner".into()))
            }
        }
        None => {
            // mint case: evidence must be the NFT's own Create action, authored by base_agent.
            if rec.action_hashed().hash != nft {
                return Ok(ValidateCallbackResult::Invalid("mint-case evidence must be the mint action".into()));
            }
            if rec.action().author() != &base_agent {
                return Ok(ValidateCallbackResult::Invalid("mint-case owner must be the minter".into()));
            }
            Ok(ValidateCallbackResult::Valid)
        }
    }
}
```

**On `DeleteLink(AgentToNft)`:** a stale ownership-index link being deleted is harmless — the
provenance chain is the ledger. Validate only that the delete targets a real `CreateLink` of this
type (`?`-propagate a shape violation, never `Invalid` — `patterns.md` rule). Optionally require the
delete author to be the link's original author.

---

## 2. API & Workflow Design (Coordinator Zome: `nft`)

`Cargo.toml`: `hdk = { workspace = true }` — **no `unstable-countersigning` feature** for Design A.

### 2.1 Function signatures

```rust
// ---------- mint_nft (minter only) ----------
#[derive(Serialize, Deserialize, Debug)]
pub struct MintNftInput { pub metadata_hash: [u8; 32] }
#[derive(Serialize, Deserialize, Debug)]
pub struct MintNftOutput { pub nft: ActionHash }   // the NFT's permanent identity
#[hdk_extern]
pub fn mint_nft(input: MintNftInput) -> ExternResult<MintNftOutput>;
// - create_entry(NftManifest { metadata_hash, payload: Sealed })
// - create_link(collection_path_hash, nft, PathToNft, ())
// - create_link(self, nft, AgentToNft, LinkTag(nft.clone()))   // mint-case evidence = mint hash
// No global counter read. No token_id.

// ---------- reveal_nft (minter only) ----------
#[derive(Serialize, Deserialize, Debug)]
pub struct RevealNftInput {
    pub nft: ActionHash,                 // original
    pub previous_action_hash: ActionHash, // patterns.md Update Pattern
    pub salt: [u8; 32],
    pub attributes: Vec<NftAttribute>,
    pub image_uri: String,
}
#[hdk_extern]
pub fn reveal_nft(input: RevealNftInput) -> ExternResult<Record>;
// update_entry(previous_action_hash, NftManifest { same metadata_hash, payload: Revealed{..} })
// create_link(nft, updated_action_hash, NftUpdates, ())

// ---------- initiate_transfer (current owner) ----------
#[derive(Serialize, Deserialize, Debug)]
pub struct InitiateTransferInput {
    pub nft: ActionHash,
    pub to_agent: AgentPubKey,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct InitiateTransferOutput { pub commitment: ActionHash }
#[hdk_extern]
pub fn initiate_transfer(input: InitiateTransferInput) -> ExternResult<InitiateTransferOutput>;
// 1. GetStrategy::Local: confirm caller owns `nft` (own AgentToNft link / own chain).
// 2. Resolve prev_state + sequence by walking NftToReceipt (GetStrategy::Network).
// 3. create_entry(TransferCommitment { nft, to_agent, prev_state, sequence }).
// 4. create_link(to_agent, commitment, AgentToInboundCommitment, ()).
// 5. send_remote_signal(TransferOffered { commitment }, vec![to_agent])  // best-effort nudge

// ---------- accept_transfer (named buyer) ----------
#[derive(Serialize, Deserialize, Debug)]
pub struct AcceptTransferInput { pub commitment: ActionHash }
#[hdk_extern]
pub fn accept_transfer(input: AcceptTransferInput) -> ExternResult<Record>;
// 1. must_get_valid_record(commitment); check caller == commitment.to_agent (belt-and-suspenders).
// 2. create_entry(TransferReceipt { commitment }).
// 3. create_link(commitment.nft, receipt_action, NftToReceipt, ()).
// 4. create_link(self, commitment.nft, AgentToNft, LinkTag(receipt_action.clone())).
// 5. send_remote_signal(TransferAccepted { commitment }, vec![seller]).

// ---------- finalize_transfer_cleanup (seller, after being signalled) ----------
#[hdk_extern]
pub fn drop_stale_ownership_link(input: AcceptTransferInput) -> ExternResult<()>;
// Seller deletes their now-stale AgentToNft link for commitment.nft. Non-atomic,
// non-authoritative — the provenance chain already reflects the new owner. If the
// seller is offline it just happens later; reads that care use §2.3 resolution.

// ---------- cancel_transfer (offerer, after window) ----------
#[hdk_extern]
pub fn cancel_transfer(input: AcceptTransferInput) -> ExternResult<Record>;
// create_entry(TransferCancellation { commitment }); delete AgentToInboundCommitment link.

// ---------- get_nfts_for_agent ----------
#[derive(Serialize, Deserialize, Debug)]
pub struct GetNftsForAgentInput { pub agent: AgentPubKey }
#[hdk_extern]
pub fn get_nfts_for_agent(input: GetNftsForAgentInput) -> ExternResult<Vec<Record>>;

// ---------- get_provenance ----------
#[hdk_extern]
pub fn get_provenance(nft: ActionHash) -> ExternResult<Vec<TransferReceipt>>; // ordered by sequence
```

### 2.2 `get_nfts_for_agent` — `GetStrategy` split

```rust
#[hdk_extern]
pub fn get_nfts_for_agent(input: GetNftsForAgentInput) -> ExternResult<Vec<Record>> {
    let me = agent_info()?.agent_initial_pubkey;
    let strategy = if input.agent == me { GetStrategy::Local } else { GetStrategy::Network };

    let links = get_links(
        LinkQuery::try_new(input.agent.clone(), LinkTypes::AgentToNft)?,
        strategy,
    )?;

    // AgentToNft is a cache; for anything authoritative, confirm against the provenance
    // chain (§2.3). For a wallet view, the cache is fine.
    let get_inputs: Vec<GetInput> = links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .map(|h| GetInput::new(h.into(), GetOptions::default()))
        .collect();
    let records = HDK.with(|hdk| hdk.borrow().get(get_inputs))?;
    Ok(records.into_iter().flatten().collect())
}
```

### 2.3 Authoritative owner resolution (coordinator helper)

```rust
pub fn current_owner(nft: ActionHash) -> ExternResult<AgentPubKey> {
    let receipts = get_links(
        LinkQuery::try_new(nft.clone(), LinkTypes::NftToReceipt)?,
        GetStrategy::Network,
    )?;
    if receipts.is_empty() {
        // No transfers yet — owner is the minter.
        let mint = must_get_valid_record(nft)?;
        return Ok(mint.action().author().clone());
    }
    // Fetch each TransferReceipt -> its TransferCommitment; pick the highest sequence.
    // Return that commitment's to_agent. (Ties are impossible: sequence is strictly
    // incremented and each state can be offered only once — enforced in validation.)
    // ... implementation walks receipts, GetOptions::default() per hop ...
    todo!()
}
```

### 2.4 `GetStrategy` vs `GetOptions` — kept distinct

The task brief's phrasing "`GetOptions::local()` vs `GetOptions::network()`" is **not the 0.7 API.**
Correct surface (from `patterns.md`):

| Call | Param type | This design |
|---|---|---|
| `get_links(LinkQuery, GetStrategy)` | `GetStrategy::{Local,Network}` | `Local` for "my" links, `Network` for others / provenance |
| `get`, `get_details`, `delete_link`, `GetInput::new` | `GetOptions` (`::default()`) | everywhere a record/link-delete is fetched |
| `must_get_*` (validation) | no strategy param | deterministic; defers on missing dep |

| Call | Strategy | Why |
|---|---|---|
| `get_nfts_for_agent(self)` | `GetStrategy::Local` | own `AgentToNft` links are self-authored; arc-0 safe |
| `get_nfts_for_agent(other)` | `GetStrategy::Network` | other agent's links aren't on your chain |
| ownership check in `initiate_transfer` | `GetStrategy::Local` | caller checks their own holdings |
| provenance walk / `current_owner` | `GetStrategy::Network` | hops authored by other agents |
| mint-count cap check | `must_get_agent_activity` in `validate()` | minter's own linear chain; no strategy param |

Nothing in Design A needs a globally-serialized read.

---

## 3. Flow Summary — Design A (no countersigning)

### Mint (minter M)
1. Client computes `salt` + metadata off-chain, `metadata_hash = blake2b(salt || canonical(attrs, image_uri))`.
2. `mint_nft(metadata_hash)` → `NftManifest{Sealed}` on M's chain. Identity = that Create action hash.
3. Same call: `PathToNft` link (discovery); `AgentToNft` link M→nft with `LinkTag = mint action hash`.
4. DHT authorities validate: minter-authority check + `max_supply` via `must_get_agent_activity` on
   M's own chain. Deterministic; no network race because a single chain is linear.

### Reveal (minter M)
5. `reveal_nft(...)` → `update_entry` on M's chain (`previous_action_hash` = mint), `NftUpdates` link.
6. `validate_update_nft_manifest` recomputes `blake2b(salt || canonical(attrs, image_uri))` and
   checks it equals the sealed `metadata_hash`. Any authority verifies independently.

### Transfer, happy path (seller S → buyer B)
7. **Offer.** S calls `initiate_transfer(nft, B)`. Coordinator confirms S owns `nft`
   (`GetStrategy::Local`), resolves `prev_state`/`sequence` from the provenance chain, commits
   `TransferCommitment{nft, to_agent: B, prev_state, sequence}` to S's chain, links
   `AgentToInboundCommitment` B→commitment, and fires a best-effort `send_remote_signal` to B.
8. **Accept (within `transfer_window_secs`).** B calls `accept_transfer(commitment)`:
   `TransferReceipt{commitment}` on B's chain; `NftToReceipt` link nft→receipt; `AgentToNft` link
   B→nft with `LinkTag = receipt action hash`. Signal to S.
9. **Validation** (independent, deterministic on each authority):
   - `validate_create_transfer_commitment`: S held the prior accepted state (`prev_state` chain);
     `sequence` increments; S has not already offered this same `prev_state` without cancelling it
     (walk of S's own chain via `must_get_agent_activity`).
   - `validate_create_transfer_receipt`: author == `commitment.to_agent`; `accepted_at <= opened_at +
     transfer_window_secs` (action timestamps); B hasn't already accepted it.
   - `validate_create_link_agent_to_nft`: the `LinkTag` receipt authorizes B as owner of `nft`
     (one `must_get_valid_record` on the exact hash — no traversal).
10. **Cleanup.** S's `post_commit`/signal handler calls `drop_stale_ownership_link` to delete S's old
    `AgentToNft` link. Non-atomic and non-authoritative: the provenance chain already names B.
11. **Query.** `get_nfts_for_agent(B)` (`Network`) shows the NFT. `current_owner(nft)` /
    `get_provenance(nft)` walk `NftToReceipt` ordered by `sequence` for the authoritative chain of
    custody.

### Transfer, buyer never accepts
- After the window, S calls `cancel_transfer(commitment)` → `TransferCancellation{commitment}` on
  S's chain (valid only *after* `opened_at + window`). S may now re-offer the same state.
- Accept-vs-cancel can't both win: acceptance is valid only *before* the deadline, cancellation only
  *after*. Disjoint by action timestamp — the one field `validate()` can rely on. Bounded clock skew
  is the only residual risk; size `transfer_window_secs` accordingly (minutes, not seconds).

---

## 4. Design B — countersigned `TransferReceipt` (optional; needs a custom conductor)

Reach for this **only** if you are already shipping a conductor built with
`unstable-countersigning`. It buys true atomicity of the *single* receipt entry across both chains;
it does **not** let you bundle link ops.

### What countersigning actually is here (from `countersigning.md`)
- Coordinator zome: `hdk = { version = "=0.7.0", features = ["unstable-countersigning"] }`.
- Session = 1 initiator + N signers, 3 beats: distribute `PreflightRequest` → each signer
  `accept_countersigning_preflight_request` (freezes that chain) → everyone commits the one entry.
- An agent can be in **one session at a time** (`PreflightRequestAcceptance::AnotherSessionIsInProgress`).
  Fine for 1:1 NFT transfers; a hot "marketplace operator" co-signing every trade would serialize.

### Signatures

```rust
// hdk = { version = "=0.7.0", features = ["unstable-countersigning"] }

#[derive(Serialize, Deserialize, Debug)]
pub struct InitiateTransferInput {
    pub nft: ActionHash,
    pub to_agent: AgentPubKey,
    pub session_ms: u64, // window; must exceed worst-case round trip for both agents
}
#[derive(Serialize, Deserialize, Debug)]
pub struct TransferSessionOffer { pub preflight_request: PreflightRequest }

#[hdk_extern]
pub fn initiate_transfer(input: InitiateTransferInput) -> ExternResult<TransferSessionOffer> {
    // 1. GetStrategy::Local: confirm caller owns `nft`; resolve prev_state + sequence.
    // 2. Build the TransferReceipt-equivalent entry bytes both sides will commit,
    //    hash it "as if not countersigned" -> app_entry_hash.
    // 3. let session_times = session_times_from_millis(input.session_ms)?;
    // 4. let req = PreflightRequest::try_new(
    //        app_entry_hash,
    //        vec![seller, buyer].into(),      // CounterSigningAgents
    //        vec![].into(),                    // no optional signers
    //        0,                                // minimum_optional_signing_agents
    //        false,                            // enzymatic
    //        session_times,
    //        action_base,                      // ActionBase::Create { .. } for this entry type
    //        preflight_bytes,                  // e.g. encode(nft, prev_state, sequence)
    //    )?;
    // 5. send req to buyer via remote call; return it to the caller's UI too.
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AcceptSessionInput { pub preflight_request: PreflightRequest }

#[hdk_extern]
pub fn accept_transfer_session(input: AcceptSessionInput) -> ExternResult<PreflightResponse> {
    match accept_countersigning_preflight_request(input.preflight_request)? {
        PreflightRequestAcceptance::Accepted(resp) => Ok(resp),
        PreflightRequestAcceptance::UnacceptableFutureStart =>
            Err(wasm_error!(WasmErrorInner::Guest("session start too far in the future".into()))),
        PreflightRequestAcceptance::UnacceptableAgentNotFound =>
            Err(wasm_error!(WasmErrorInner::Guest("not a party to this session".into()))),
        PreflightRequestAcceptance::AnotherSessionIsInProgress =>
            Err(wasm_error!(WasmErrorInner::Guest("retry: another session in progress".into()))),
        PreflightRequestAcceptance::Invalid(s) =>
            Err(wasm_error!(WasmErrorInner::Guest(format!("preflight invalid: {s}")))),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitSessionInput {
    pub responses: Vec<PreflightResponse>, // one per signer, exchanged out of band
}

#[hdk_extern]
pub fn commit_transfer(input: CommitSessionInput) -> ExternResult<Record> {
    // Called by BOTH agents. Build Entry::CounterSign session data from `responses`,
    // then create_entry the shared TransferReceipt entry. Session lands both actions
    // or times out and neither commits. AFTER it lands, each side separately does its
    // own AgentToNft link maintenance (NOT part of the session).
}
```

Also handle `ZomeCallResponse::CountersigningSession(String)` explicitly wherever these are called
cross-zome (it means "the session failed to start", distinct from `NetworkError`).

### `validate()` under Design B
- **Do not try to detect `Entry::CounterSign` in `validate()`.** `countersigning.md` documents no
  `hdi` accessor for it, and states the conductor's system validation runs
  `check_countersigning_session_data` *before* app validation — so session legitimacy (both named
  agents really co-signed) is a host guarantee.
- Your `validate_create_transfer_receipt` still checks the same **semantic** rules as Design A
  (author is a party, `prev_state` chain, `sequence`), minus the timestamp-window race logic —
  atomicity removes the accept-vs-cancel race entirely.

### Flow (seller S, buyer B)
1. **Preflight.** S: `initiate_transfer` → `PreflightRequest` (signers `[S, B]`, one entry:
   the `TransferReceipt`). Sent to B.
2. **Accept.** B: `accept_transfer_session` → `accept_countersigning_preflight_request` freezes B's
   chain, returns `PreflightResponse`. S does the same. Responses exchanged.
3. **Commit.** Both call `commit_transfer`; each commits the identical `TransferReceipt`, wrapped by
   the conductor as `Entry::CounterSign`. Both land or the session times out.
4. **Index.** Each side separately writes its `AgentToNft` / `NftToReceipt` links (non-atomic).
5. **Query.** Same as Design A.

---

## 5. Retained from the original plan (unchanged, validated against the refs)

- No `owner` field in `NftManifest`; ownership is DHT relationship state.
- `timestamp` / `author` stay in the action header, never entry fields (`patterns.md`).
- Commit-reveal blind minting with a deterministic preimage check in `validate()`.
- Collection config in DNA `modifiers.properties`, read via `dna_info()` — safe in validation
  (`progenitor.md`), never `init_properties`.
- Original mint action hash is the stable NFT identity and provenance anchor.
- `royalty_bps: u16` present but unenforced — v2 seam, no schema migration needed to activate.
- One DNA per collection, provisioned as clone cells (`cell-cloning.md`); `max_supply` etc. in
  DNA properties.

---

## 6. Entry / link summary table

| Entry | Author | Links out | Update chain? | Discovery | Validation crux |
|---|---|---|---|---|---|
| `NftManifest` | minter | `PathToNft`, `AgentToNft` (seq 0) | Yes (`NftUpdates`, reveal only) | Path + Agent | minter-authority + `max_supply` via `must_get_agent_activity` |
| `TransferCommitment` | current owner | `AgentToInboundCommitment` | No (immutable) | per-buyer inbox | seller holds prior state; state offered once |
| `TransferReceipt` | named buyer | `NftToReceipt`, `AgentToNft` | No | via `NftToReceipt` | author == `to_agent`; within window; not double-accepted |
| `TransferCancellation` | offerer | — | No | — | offerer only; after window only |

---

## 7. Open items to spike in Sweettest before writing the rest

1. **`must_get_agent_activity` return shape + entry-type filtering.** The `max_supply` count and the
   "state offered only once" / "not double-accepted" checks all iterate an agent's own chain slice
   and filter by app entry type. `countersigning.md` / `patterns.md` confirm `must_get_agent_activity`
   is allowed and deterministic in validation, but the exact accessor for reading an entry's
   `AppEntryDef` off each returned activity record in `hdi 0.8` is unconfirmed by the references —
   write a 2-agent Sweettest that mints past `max_supply` and asserts the reject.
2. **`ChainFilter::new(prev)` walk direction.** `patterns.md` says 0.7 `must_get_agent_activity`
   "walks down from the `chain_top` you give it, excludes forked actions". Confirm `ChainFilter::new`
   (no extra bound) returns genesis→`prev` inclusive as assumed here.
3. **Clock-skew sizing for `transfer_window_secs`.** Test acceptance and cancellation with signer
   clocks deliberately skewed; pick a window that keeps the two verdicts disjoint under realistic
   skew (start at 300s).
4. **`LinkTag` round-trip as `ActionHash`.** `validate_create_link_agent_to_nft` reads the tag bytes
   back as an `ActionHash`. Confirm `LinkTag::new(hash.clone().into_inner())` /
   `ActionHash::try_from(tag.into_inner())` round-trips cleanly.
5. **(Design B only)** `ActionBase` construction for a `Create` of `TransferReceipt`, and the exact
   `CounterSigningAgents` / `PreflightBytes` constructors — only spike this if you commit to a custom
   conductor.

---

## 8. Recommendation

Scaffold **Design A**. It needs no feature gate, no custom conductor, no chain freeze, and it is what
`countersigning.md`'s own design checklist points to first ("expressed as validation on two
independent entries instead"). Keep Design B in this doc as the upgrade path if a future requirement
(e.g. atomic swap of NFT-for-payment in one session) genuinely needs host-level atomicity — at which
point the custom-conductor cost is a deliberate, budgeted decision.
