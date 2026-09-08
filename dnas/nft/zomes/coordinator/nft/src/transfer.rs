use crate::query::next_transfer_state;
use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

// --- initiate_transfer (current owner) --------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct InitiateTransferInput {
    pub nft: ActionHash,
    pub to_agent: AgentPubKey,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitiateTransferOutput {
    pub commitment: ActionHash,
}

#[hdk_extern]
pub fn initiate_transfer(input: InitiateTransferInput) -> ExternResult<InitiateTransferOutput> {
    let me = agent_info()?.agent_initial_pubkey;

    // Fast local pre-filter: do I even have an ownership-index link for this NFT?
    // (Can be stale — the authoritative check is next.)
    let my_links = get_links(
        LinkQuery::try_new(me.clone(), LinkTypes::AgentToNft)?,
        GetStrategy::Local,
    )?;
    let indexed = my_links
        .iter()
        .filter_map(|l| l.target.clone().into_action_hash())
        .any(|h| h == input.nft);
    if !indexed {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "caller does not currently own this NFT".into()
        )));
    }
    // Authoritative: the provenance chain, not the (possibly stale) index link.
    if crate::query::current_owner(input.nft.clone())? != me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "caller is not the current owner per the provenance chain".into()
        )));
    }

    let (prev_state, sequence) = next_transfer_state(&input.nft)?;

    let commitment = TransferCommitment {
        nft: input.nft.clone(),
        to_agent: input.to_agent.clone(),
        prev_state,
        sequence,
    };
    let commitment_hash = create_entry(&EntryTypes::TransferCommitment(commitment))?;

    // Route the offer into the buyer's inbox.
    create_link(
        input.to_agent.clone(),
        commitment_hash.clone(),
        LinkTypes::AgentToInboundCommitment,
        (),
    )?;

    // Best-effort nudge (not load-bearing — the inbox link is the durable path).
    let _ = send_remote_signal(
        RemoteSignal::TransferOffered {
            commitment: commitment_hash.clone(),
        },
        vec![input.to_agent],
    );

    Ok(InitiateTransferOutput {
        commitment: commitment_hash,
    })
}

// --- accept_transfer (named buyer) ----------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitmentRef {
    pub commitment: ActionHash,
}

#[hdk_extern]
pub fn accept_transfer(input: CommitmentRef) -> ExternResult<Record> {
    let me = agent_info()?.agent_initial_pubkey;

    let commitment_rec = get(input.commitment.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("commitment not found".into())
    ))?;
    let commitment: TransferCommitment = commitment_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a TransferCommitment".into()
        )))?;
    let seller = commitment_rec.action().author().clone();

    if commitment.to_agent != me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "only the named buyer may accept this transfer".into()
        )));
    }

    let receipt_hash = create_entry(&EntryTypes::TransferReceipt(TransferReceipt {
        commitment: input.commitment.clone(),
    }))?;

    // Append-only provenance.
    create_link(
        commitment.nft.clone(),
        receipt_hash.clone(),
        LinkTypes::NftToReceipt,
        (),
    )?;
    // Ownership index for the buyer; evidence tag = this receipt's action hash.
    create_link(
        me,
        commitment.nft.clone(),
        LinkTypes::AgentToNft,
        LinkTag::new(receipt_hash.clone().into_inner()),
    )?;

    let _ = send_remote_signal(
        RemoteSignal::TransferAccepted {
            commitment: input.commitment.clone(),
        },
        vec![seller],
    );

    get(receipt_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created TransferReceipt not found".into()
    )))
}

// --- cancel_transfer (offerer, after the window) --------------------------

#[hdk_extern]
pub fn cancel_transfer(input: CommitmentRef) -> ExternResult<Record> {
    let me = agent_info()?.agent_initial_pubkey;

    let commitment_rec = get(input.commitment.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("commitment not found".into())
    ))?;
    if commitment_rec.action().author() != &me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "only the offerer may cancel this commitment".into()
        )));
    }
    let commitment: TransferCommitment = commitment_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a TransferCommitment".into()
        )))?;

    let cancellation_hash = create_entry(&EntryTypes::TransferCancellation(TransferCancellation {
        commitment: input.commitment.clone(),
    }))?;

    // Remove the offer from the buyer's inbox (seller authored that link).
    let inbox = get_links(
        LinkQuery::try_new(commitment.to_agent.clone(), LinkTypes::AgentToInboundCommitment)?,
        GetStrategy::Network,
    )?;
    for link in inbox {
        if link.target.clone().into_action_hash() == Some(input.commitment.clone()) {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }

    get(cancellation_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created TransferCancellation not found".into()
    )))
}

// --- drop_stale_ownership_link (seller, after being told the sale landed) ---

#[hdk_extern]
pub fn drop_stale_ownership_link(nft: ActionHash) -> ExternResult<()> {
    let me = agent_info()?.agent_initial_pubkey;
    let my_links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToNft)?,
        GetStrategy::Local,
    )?;
    for link in my_links {
        if link.target.clone().into_action_hash() == Some(nft.clone()) {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    Ok(())
}

// --- inbox read -----------------------------------------------------------

#[hdk_extern]
pub fn get_inbound_commitments(_: ()) -> ExternResult<Vec<Record>> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToInboundCommitment)?,
        GetStrategy::Local,
    )?;
    let get_inputs: Vec<GetInput> = links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .map(|h| GetInput::new(h.into(), GetOptions::default()))
        .collect();
    let records = HDK.with(|hdk| hdk.borrow().get(get_inputs))?;
    Ok(records.into_iter().flatten().collect())
}
