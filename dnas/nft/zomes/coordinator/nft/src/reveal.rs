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
}

#[hdk_extern]
pub fn reveal_nft(input: RevealNftInput) -> ExternResult<Record> {
    let original = get(input.nft.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("NftManifest not found".into())
    ))?;

    let me = agent_info()?.agent_initial_pubkey;
    if original.action().author() != &me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "only the minter may reveal this NFT".into()
        )));
    }

    let original_manifest: NftManifest = original
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "original entry is not an NftManifest".into()
        )))?;

    // Fail fast client-side too: the revealed data must match the commitment.
    let computed = commit_hash(&reveal_preimage(
        &input.salt,
        &input.attributes,
        &input.image_uri,
    ));
    if computed != original_manifest.metadata_hash {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "revealed metadata does not match the mint commitment".into()
        )));
    }

    let revealed = NftManifest {
        metadata_hash: original_manifest.metadata_hash,
        payload: NftPayload::Revealed {
            salt: input.salt,
            attributes: input.attributes,
            image_uri: input.image_uri,
        },
    };

    let updated_hash = update_entry(input.previous_action_hash, &revealed)?;
    create_link(
        input.nft,
        updated_hash.clone(),
        LinkTypes::NftUpdates,
        (),
    )?;

    get(updated_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "revealed NftManifest not found".into()
    )))
}
