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
}

#[hdk_extern]
pub fn mint_nft(input: MintNftInput) -> ExternResult<MintNftOutput> {
    let nft = NftManifest {
        metadata_hash: input.metadata_hash,
        payload: NftPayload::Sealed,
    };
    let nft_hash = create_entry(&EntryTypes::NftManifest(nft))?;

    // Discovery anchor. The anchor path entry itself need not be committed — readers
    // recompute `Path::from(COLLECTION_PATH).path_entry_hash()` to query these links.
    let path = Path::from(COLLECTION_PATH);
    create_link(
        path.path_entry_hash()?,
        nft_hash.clone(),
        LinkTypes::PathToNft,
        (),
    )?;

    // Ownership index, mint case: the authorizing evidence in the LinkTag is the
    // mint action itself. `validate_create_link_agent_to_nft` checks base == author.
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
    Ok(MintNftOutput {
        nft: nft_hash,
        record,
    })
}

/// UI helper: may the calling agent mint in this collection?
#[hdk_extern]
pub fn is_minter(_: ()) -> ExternResult<bool> {
    match CollectionProperties::minter()? {
        None => Ok(true), // open minting (dev)
        Some(minter) => Ok(minter == agent_info()?.agent_initial_pubkey),
    }
}
