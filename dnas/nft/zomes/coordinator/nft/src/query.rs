use hdk::prelude::*;
use nft_integrity::*;

/// This collection's configuration, read from DNA `modifiers.properties`. UIs need
/// `transfer_window_secs` (offer countdown) and `name` / `symbol` / `max_supply`.
#[hdk_extern]
pub fn collection_info(_: ()) -> ExternResult<CollectionProperties> {
    CollectionProperties::get()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetNftsForAgentInput {
    pub agent: AgentPubKey,
}

/// Wallet view. `AgentToNft` is a derived index — fine for display. For anything
/// authoritative use [`current_owner`].
#[hdk_extern]
pub fn get_nfts_for_agent(input: GetNftsForAgentInput) -> ExternResult<Vec<Record>> {
    let me = agent_info()?.agent_initial_pubkey;
    let strategy = if input.agent == me {
        GetStrategy::Local
    } else {
        GetStrategy::Network
    };

    let links = get_links(
        LinkQuery::try_new(input.agent, LinkTypes::AgentToNft)?,
        strategy,
    )?;

    let get_inputs: Vec<GetInput> = links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .map(|h| GetInput::new(h.into(), GetOptions::default()))
        .collect();
    let records = HDK.with(|hdk| hdk.borrow().get(get_inputs))?;
    Ok(records.into_iter().flatten().collect())
}

/// Latest version of the manifest (walks the `NftUpdates` reveal chain).
#[hdk_extern]
pub fn get_nft(nft: ActionHash) -> ExternResult<Option<Record>> {
    let links = get_links(
        LinkQuery::try_new(nft.clone(), LinkTypes::NftUpdates)?,
        GetStrategy::Network,
    )?;
    let latest = links
        .into_iter()
        .max_by(|a, b| a.timestamp.cmp(&b.timestamp))
        .and_then(|l| l.target.into_action_hash())
        .unwrap_or(nft);
    get(latest, GetOptions::default())
}

/// A resolved provenance hop.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProvenanceHop {
    pub sequence: u32,
    pub from_agent: AgentPubKey,
    pub to_agent: AgentPubKey,
    pub receipt_action: ActionHash,
    pub commitment_action: ActionHash,
}

fn resolve_hops(nft: &ActionHash) -> ExternResult<Vec<ProvenanceHop>> {
    let receipt_links = get_links(
        LinkQuery::try_new(nft.clone(), LinkTypes::NftToReceipt)?,
        GetStrategy::Network,
    )?;

    let mint = must_get_valid_record(nft.clone())?;
    let minter = mint.action().author().clone();

    let mut hops: Vec<ProvenanceHop> = Vec::new();
    for link in receipt_links {
        let Some(receipt_action) = link.target.into_action_hash() else {
            continue;
        };
        let receipt_rec = get(receipt_action.clone(), GetOptions::default())?.ok_or(wasm_error!(
            WasmErrorInner::Guest("provenance: receipt record missing".into())
        ))?;
        let receipt: TransferReceipt = receipt_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "provenance: not a TransferReceipt".into()
            )))?;
        let commitment_rec = get(receipt.commitment.clone(), GetOptions::default())?.ok_or(
            wasm_error!(WasmErrorInner::Guest(
                "provenance: commitment record missing".into()
            )),
        )?;
        let commitment: TransferCommitment = commitment_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "provenance: not a TransferCommitment".into()
            )))?;

        hops.push(ProvenanceHop {
            sequence: commitment.sequence,
            from_agent: commitment_rec.action().author().clone(),
            to_agent: commitment.to_agent.clone(),
            receipt_action,
            commitment_action: receipt.commitment.clone(),
        });
    }

    hops.sort_by_key(|h| h.sequence);

    // Sanity: the chain must start at 1 and increment by 1. A gap or duplicate
    // means an unresolved/forked provenance — surface it rather than guessing.
    for (i, hop) in hops.iter().enumerate() {
        if hop.sequence != (i as u32) + 1 {
            return Err(wasm_error!(WasmErrorInner::Guest(format!(
                "provenance chain for {nft:?} is not contiguous at sequence {}",
                hop.sequence
            ))));
        }
    }
    let _ = minter; // reserved for future cross-checks
    Ok(hops)
}

#[hdk_extern]
pub fn get_provenance(nft: ActionHash) -> ExternResult<Vec<ProvenanceHop>> {
    resolve_hops(&nft)
}

/// Authoritative current owner: the `to_agent` of the highest-sequence accepted
/// transfer, or the minter if there are none.
#[hdk_extern]
pub fn current_owner(nft: ActionHash) -> ExternResult<AgentPubKey> {
    let hops = resolve_hops(&nft)?;
    match hops.last() {
        Some(hop) => Ok(hop.to_agent.clone()),
        None => {
            let mint = must_get_valid_record(nft)?;
            Ok(mint.action().author().clone())
        }
    }
}

/// `(prev_state, next_sequence)` for the next `TransferCommitment` on this NFT.
pub fn next_transfer_state(nft: &ActionHash) -> ExternResult<(ActionHash, u32)> {
    let hops = resolve_hops(nft)?;
    match hops.last() {
        Some(hop) => Ok((hop.receipt_action.clone(), hop.sequence + 1)),
        None => Ok((nft.clone(), 1)),
    }
}
