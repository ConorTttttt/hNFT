use crate::COLLECTION_PATH;
use hdk::prelude::*;
use nft_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct MintNftInput {
    /// `blake2b_256(salt || canonical_bytes(attributes, image_uri))`, computed
    /// client-side. The salt + metadata stay off-chain until `reveal_nft`.
    pub metadata_hash: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MintNftOutput {
    /// The NFT's permanent identity: its mint Create action hash.
    pub nft: ActionHash,
    pub record: Record,
    /// The `MintAgreement` action, when minted with one.
    #[serde(default)]
    pub agreement: Option<ActionHash>,
}

/// The reveal preimage a minter parks (privately, on-chain) so the scheduled
/// auto-reveal sweep can reveal without the browser's saved copy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RevealSecretInput {
    pub salt: [u8; 32],
    pub attributes: Vec<NftAttribute>,
    pub image_uri: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MintWithAgreementInput {
    pub metadata_hash: [u8; 32],
    /// Microseconds since the epoch. After this a `delegate` may reveal.
    pub reveal_by_micros: i64,
    /// Microseconds since the epoch. After this anyone may mark it defaulted.
    pub default_after_micros: i64,
    #[serde(default)]
    pub delegate: Option<AgentPubKey>,
    /// When present, parked as a private `RevealSecret` and the auto-reveal
    /// scheduler is started.
    #[serde(default)]
    pub secret: Option<RevealSecretInput>,
}

/// Create the sealed manifest + its discovery / ownership-index links.
fn mint_manifest(metadata_hash: [u8; 32]) -> ExternResult<(ActionHash, Record)> {
    mint_manifest_in_batch(metadata_hash, None, None)
}

/// Same as [`mint_manifest`], but able to tag the mint with its blind-box
/// batch membership. `batch`/`batch_slot` must be both `Some` or both `None`.
pub(crate) fn mint_manifest_in_batch(
    metadata_hash: [u8; 32],
    batch: Option<ActionHash>,
    batch_slot: Option<u32>,
) -> ExternResult<(ActionHash, Record)> {
    let nft = NftManifest {
        metadata_hash,
        payload: NftPayload::Sealed,
        batch,
        batch_slot,
    };
    let nft_hash = create_entry(&EntryTypes::NftManifest(nft))?;

    let path = Path::from(COLLECTION_PATH);
    create_link(
        path.path_entry_hash()?,
        nft_hash.clone(),
        LinkTypes::PathToNft,
        (),
    )?;

    let me = agent_info()?.agent_initial_pubkey;
    create_link(
        me,
        nft_hash.clone(),
        LinkTypes::AgentToNft,
        LinkTag::new(nft_hash.clone().into_inner()),
    )?;

    let record = get(nft_hash.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("newly minted NftManifest not found".into())
    ))?;
    Ok((nft_hash, record))
}

#[hdk_extern]
pub fn mint_nft(input: MintNftInput) -> ExternResult<MintNftOutput> {
    let (nft, record) = mint_manifest(input.metadata_hash)?;
    Ok(MintNftOutput {
        nft,
        record,
        agreement: None,
    })
}

#[hdk_extern]
pub fn mint_with_agreement(input: MintWithAgreementInput) -> ExternResult<MintNftOutput> {
    let (nft, _) = mint_manifest(input.metadata_hash)?;

    let agreement = MintAgreement {
        nft: nft.clone(),
        reveal_by: Timestamp::from_micros(input.reveal_by_micros),
        default_after: Timestamp::from_micros(input.default_after_micros),
        delegate: input.delegate.clone(),
        payment_ref: None,
    };
    let agreement_hash = create_entry(&EntryTypes::MintAgreement(agreement))?;
    create_link(
        nft.clone(),
        agreement_hash.clone(),
        LinkTypes::NftToAgreement,
        (),
    )?;

    if let Some(delegate) = input.delegate {
        create_link(
            delegate.clone(),
            agreement_hash.clone(),
            LinkTypes::AgentToDelegatedReveal,
            (),
        )?;
        let _ = send_remote_signal(
            crate::RemoteSignal::DelegatedReveal {
                nft: nft.clone(),
                agreement: agreement_hash.clone(),
            },
            vec![delegate],
        );
    }

    if let Some(secret) = input.secret {
        create_entry(&EntryTypes::RevealSecret(RevealSecret {
            nft: nft.clone(),
            salt: secret.salt,
            attributes: secret.attributes,
            image_uri: secret.image_uri,
        }))?;
        let _ = schedule("auto_reveal_due");
    }

    let record = get(nft.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("newly minted NftManifest not found".into())
    ))?;
    Ok(MintNftOutput {
        nft,
        record,
        agreement: Some(agreement_hash),
    })
}

/// UI helper (Phase J.2): may the calling agent mint in this collection RIGHT
/// NOW, and what does their personal cap/progress look like? A read-only
/// reflection of authoritative state — never a second authorization decision.
/// The actual gate for every mint is still the integrity validator, which
/// resolves `mint_mode()` and reaches `eligible`'s answer independently.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MintPolicyView {
    Public,
    CreatorOnly,
    Whitelist,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MintStatus {
    pub policy: MintPolicyView,
    /// Mirrors what `is_minter()` used to return standalone — the
    /// authoritative "may I mint right now" answer.
    pub eligible: bool,
    /// This agent's own lifetime `NftManifest` mint count, from their own
    /// local chain (no network call) — meaningful under every policy, not
    /// just WHITELIST.
    pub minted_by_me: u32,
    /// This agent's personal cap, when one applies to them specifically:
    /// their whitelist allowance, or — only when THIS agent IS the fixed
    /// creator — the collection's `max_supply`. `None` under PUBLIC (no
    /// personal cap exists), for a non-creator under CREATOR_ONLY, or for an
    /// agent absent from the whitelist under WHITELIST.
    pub my_allowance: Option<u32>,
}

#[hdk_extern]
pub fn mint_status(_: ()) -> ExternResult<MintStatus> {
    let me = agent_info()?.agent_initial_pubkey;
    let minted_by_me = count_my_manifest_mints()?;
    let props = CollectionProperties::get()?;
    match props.mint_mode()? {
        MintMode::Public => Ok(MintStatus {
            policy: MintPolicyView::Public,
            eligible: true,
            minted_by_me,
            my_allowance: None,
        }),
        MintMode::CreatorOnly(minter) => {
            let is_creator = minter == me;
            Ok(MintStatus {
                policy: MintPolicyView::CreatorOnly,
                eligible: is_creator,
                minted_by_me,
                my_allowance: (is_creator && props.max_supply > 0).then_some(props.max_supply),
            })
        }
        MintMode::Whitelist(entries) => {
            let my_allowance = entries.iter().find(|(a, _)| *a == me).map(|(_, n)| *n);
            let eligible = my_allowance.is_some_and(|n| minted_by_me < n);
            Ok(MintStatus {
                policy: MintPolicyView::Whitelist,
                eligible,
                minted_by_me,
                my_allowance,
            })
        }
    }
}

/// UI helper: may the calling agent mint in this collection RIGHT NOW? Kept
/// for existing/simple callers; `mint_status` returns the same answer plus
/// allowance/progress detail.
#[hdk_extern]
pub fn is_minter(_: ()) -> ExternResult<bool> {
    Ok(mint_status(())?.eligible)
}

/// Counts the calling agent's own prior `NftManifest` creates, over their own
/// local chain (`query()` — no network round-trip; local-only, same as the
/// `GetStrategy::Local` pattern this codebase already uses for own-data reads
/// elsewhere). Used only for the UI-facing `is_minter` reflection; the
/// authoritative count enforced at mint time is computed independently, over
/// `must_get_agent_activity`, in `validate_create_nft_manifest`.
fn count_my_manifest_mints() -> ExternResult<u32> {
    let records = query(ChainQueryFilter::new())?;
    Ok(records
        .iter()
        .filter(|r| is_nft_manifest_create(r.action()))
        .count() as u32)
}
