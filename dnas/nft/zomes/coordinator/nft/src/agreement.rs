use crate::reveal::{agreement_hash_for, is_defaulted, load_agreement};
use hdk::prelude::*;
use nft_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct AgreementView {
    pub agreement: ActionHash,
    pub reveal_by_micros: i64,
    pub default_after_micros: i64,
    pub delegate: Option<AgentPubKey>,
    pub defaulted: bool,
    /// Is the connected agent the agreement's delegate?
    pub am_delegate: bool,
}

/// The `MintAgreement` for an NFT (if any), flattened for the UI.
#[hdk_extern]
pub fn agreement_for(nft: ActionHash) -> ExternResult<Option<AgreementView>> {
    let Some(ag_hash) = agreement_hash_for(&nft)? else {
        return Ok(None);
    };
    let ag = load_agreement(&ag_hash)?;
    let me = agent_info()?.agent_initial_pubkey;
    Ok(Some(AgreementView {
        agreement: ag_hash,
        reveal_by_micros: ag.reveal_by.as_micros(),
        default_after_micros: ag.default_after.as_micros(),
        delegate: ag.delegate.clone(),
        defaulted: is_defaulted(&nft)?,
        am_delegate: ag.delegate.as_ref() == Some(&me),
    }))
}

/// Mark an agreement's reveal as defaulted. Valid only after `default_after`
/// (enforced in `validate_create_reveal_default`).
#[hdk_extern]
pub fn mark_reveal_default(nft: ActionHash) -> ExternResult<Record> {
    let ag_hash = agreement_hash_for(&nft)?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "this NFT has no MintAgreement".into()
    )))?;
    let default_hash = create_entry(&EntryTypes::RevealDefault(RevealDefault {
        nft: nft.clone(),
        agreement: ag_hash,
    }))?;
    create_link(nft, default_hash.clone(), LinkTypes::NftToRevealDefault, ())?;
    get(default_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created RevealDefault not found".into()
    )))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DelegatedRevealView {
    pub nft: ActionHash,
    pub agreement: ActionHash,
    pub reveal_by_micros: i64,
    pub default_after_micros: i64,
    pub defaulted: bool,
    pub sealed: bool,
}

/// NFTs the connected agent is the reveal delegate for.
#[hdk_extern]
pub fn my_delegated_reveals(_: ()) -> ExternResult<Vec<DelegatedRevealView>> {
    let me = agent_info()?.agent_initial_pubkey;
    // The AgentToDelegatedReveal link is authored by the minter, base = delegate.
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToDelegatedReveal)?,
        GetStrategy::Network,
    )?;
    let mut out = Vec::new();
    for l in links {
        let Some(ag_hash) = l.target.into_action_hash() else {
            continue;
        };
        let Ok(ag) = load_agreement(&ag_hash) else {
            continue;
        };
        let sealed = match crate::query::get_nft(ag.nft.clone())? {
            Some(rec) => matches!(
                rec.entry()
                    .to_app_option::<NftManifest>()
                    .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
                    .map(|m| m.payload),
                Some(NftPayload::Sealed)
            ),
            None => true,
        };
        out.push(DelegatedRevealView {
            nft: ag.nft.clone(),
            agreement: ag_hash,
            reveal_by_micros: ag.reveal_by.as_micros(),
            default_after_micros: ag.default_after.as_micros(),
            defaulted: is_defaulted(&ag.nft)?,
            sealed,
        });
    }
    Ok(out)
}

/// The private reveal secret this device parked for an NFT, if any. Only the
/// minter's own conductor can answer this (the entry is private to its chain).
#[hdk_extern]
pub fn reveal_secret_for(nft: ActionHash) -> ExternResult<Option<crate::mint::RevealSecretInput>> {
    let records = query(ChainQueryFilter::new().include_entries(true))?;
    for rec in records {
        if let Ok(Some(secret)) = rec.entry().to_app_option::<RevealSecret>() {
            if secret.nft == nft {
                return Ok(Some(crate::mint::RevealSecretInput {
                    salt: secret.salt,
                    attributes: secret.attributes,
                    image_uri: secret.image_uri,
                }));
            }
        }
    }
    Ok(None)
}
