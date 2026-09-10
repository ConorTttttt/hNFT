use crate::mint::RevealSecretInput;
use crate::query::current_owner;
use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct RevealNftInput {
    /// The NFT's identity (mint Create action).
    pub nft: ActionHash,
    /// Head of this agent's reveal chain for the NFT — the mint action for a
    /// first reveal (per the Update Pattern in patterns.md).
    pub previous_action_hash: ActionHash,
    pub salt: [u8; 32],
    pub attributes: Vec<NftAttribute>,
    pub image_uri: String,
    /// Reveal even though a `RevealDefault` exists (a late reveal).
    #[serde(default)]
    pub force: bool,
}

// --- helpers shared with the auto-reveal scheduler --------------------------

pub(crate) fn agreement_hash_for(nft: &ActionHash) -> ExternResult<Option<ActionHash>> {
    Ok(get_links(
        LinkQuery::try_new(nft.clone(), LinkTypes::NftToAgreement)?,
        GetStrategy::Network,
    )?
    .into_iter()
    .next()
    .and_then(|l| l.target.into_action_hash()))
}

pub(crate) fn load_agreement(hash: &ActionHash) -> ExternResult<MintAgreement> {
    get(hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "MintAgreement not found".into()
        )))?
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "not a MintAgreement".into()
        )))
}

pub(crate) fn is_defaulted(nft: &ActionHash) -> ExternResult<bool> {
    Ok(!get_links(
        LinkQuery::try_new(nft.clone(), LinkTypes::NftToRevealDefault)?,
        GetStrategy::Network,
    )?
    .is_empty())
}

/// Shared reveal core. `previous_action_hash` is the caller's own chain head for
/// this NFT — the mint action for a first reveal.
pub(crate) fn do_reveal(
    nft: ActionHash,
    previous_action_hash: ActionHash,
    secret: RevealSecretInput,
) -> ExternResult<Record> {
    let original = get(nft.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("NftManifest not found".into())
    ))?;
    let original_manifest: NftManifest = original
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "original entry is not an NftManifest".into()
        )))?;

    let computed = commit_hash(&reveal_preimage(
        &secret.salt,
        &secret.attributes,
        &secret.image_uri,
    ));
    if computed != original_manifest.metadata_hash {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "revealed metadata does not match the mint commitment".into()
        )));
    }

    let revealed = NftManifest {
        metadata_hash: original_manifest.metadata_hash,
        payload: NftPayload::Revealed {
            salt: secret.salt,
            attributes: secret.attributes,
            image_uri: secret.image_uri,
            agreement: agreement_hash_for(&nft)?,
        },
    };

    let updated_hash = update_entry(previous_action_hash, &revealed)?;
    create_link(nft.clone(), updated_hash.clone(), LinkTypes::NftUpdates, ())?;

    let me = agent_info()?.agent_initial_pubkey;
    if let Ok(owner) = current_owner(nft.clone()) {
        if owner != me {
            let _ = send_remote_signal(RemoteSignal::NftRevealed { nft: nft.clone() }, vec![owner]);
        }
    }

    get(updated_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "revealed NftManifest not found".into()
    )))
}

#[hdk_extern]
pub fn reveal_nft(input: RevealNftInput) -> ExternResult<Record> {
    let me = agent_info()?.agent_initial_pubkey;
    let original = get(input.nft.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("NftManifest not found".into())
    ))?;
    let minter = original.action().author().clone();

    if me != minter {
        // Delegate path: allowed only if a MintAgreement names me and reveal_by
        // has passed. (validate() re-checks this deterministically.)
        let ag_hash = agreement_hash_for(&input.nft)?.ok_or(wasm_error!(
            WasmErrorInner::Guest("only the minter may reveal this NFT".into())
        ))?;
        let ag = load_agreement(&ag_hash)?;
        if ag.delegate.as_ref() != Some(&me) {
            return Err(wasm_error!(WasmErrorInner::Guest(
                "you are not this NFT's reveal delegate".into()
            )));
        }
        if sys_time()?.as_micros() < ag.reveal_by.as_micros() {
            return Err(wasm_error!(WasmErrorInner::Guest(
                "the delegate may reveal only after reveal_by".into()
            )));
        }
    }

    if !input.force && is_defaulted(&input.nft)? {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "this NFT's reveal has defaulted — pass force to reveal anyway".into()
        )));
    }

    do_reveal(
        input.nft,
        input.previous_action_hash,
        RevealSecretInput {
            salt: input.salt,
            attributes: input.attributes,
            image_uri: input.image_uri,
        },
    )
}
