pub mod agreement;
pub mod listing;
pub mod mint;
pub mod payment;
pub mod query;
pub mod reveal;
pub mod transfer;

use hdk::prelude::*;
use nft_integrity::*;

/// Path anchor for global NFT discovery within a collection cell.
pub const COLLECTION_PATH: &str = "collection.all";

/// Auto-reveal sweep cadence (7-field crontab: sec min hour dom mon dow year).
const AUTO_REVEAL_CRON: &str = "0 */5 * * * * *";

// Called the first time a zome call is made to the cell containing this zome.
#[hdk_extern]
pub fn init() -> ExternResult<InitCallbackResult> {
    // Allow any agent to deliver transfer-handshake nudges to this agent.
    let mut functions = HashSet::new();
    functions.insert((zome_info()?.name, FunctionName::from("recv_remote_signal")));
    create_cap_grant(ZomeCallCapGrant {
        tag: "recv_remote_signal".into(),
        access: CapAccess::Unrestricted,
        functions: GrantedFunctions::Listed(functions),
    })?;
    // Best effort — also (re)started by mint_with_agreement. `init` is lazy.
    let _ = schedule("auto_reveal_due");
    Ok(InitCallbackResult::Pass)
}

#[hdk_extern]
pub fn start_auto_reveal(_: ()) -> ExternResult<()> {
    schedule("auto_reveal_due")
}

/// Reveal this agent's own sealed NFTs whose `MintAgreement.reveal_by` has passed,
/// using the private `RevealSecret` parked at mint. Runs as the chain author
/// (scheduling.md rule 1); only fires while this conductor is up.
#[hdk_extern(infallible)]
fn auto_reveal_due(_previous: Option<Schedule>) -> Option<Schedule> {
    let keep = Some(Schedule::Persisted(AUTO_REVEAL_CRON.to_string()));

    let Ok(now) = sys_time().map(|t| t.as_micros()) else {
        return keep;
    };
    let Ok(records) = query(ChainQueryFilter::new().include_entries(true)) else {
        return keep;
    };

    for rec in records {
        let Ok(Some(secret)) = rec.entry().to_app_option::<RevealSecret>() else {
            continue;
        };
        let nft = secret.nft.clone();

        let still_sealed = matches!(
            crate::query::get_nft(nft.clone())
                .ok()
                .flatten()
                .and_then(|r| r.entry().to_app_option::<NftManifest>().ok().flatten())
                .map(|m| m.payload),
            Some(NftPayload::Sealed)
        );
        if !still_sealed {
            continue;
        }

        let Ok(Some(ag_hash)) = crate::reveal::agreement_hash_for(&nft) else {
            continue;
        };
        let Ok(ag) = crate::reveal::load_agreement(&ag_hash) else {
            continue;
        };
        // Defensive: only act inside the intended window (scheduling.md rule 4).
        if now < ag.reveal_by.as_micros() {
            continue;
        }
        if crate::reveal::is_defaulted(&nft).unwrap_or(true) {
            continue;
        }

        // First reveal branches off the mint action (prev == nft).
        let _ = crate::reveal::do_reveal(
            nft.clone(),
            nft.clone(),
            crate::mint::RevealSecretInput {
                salt: secret.salt,
                attributes: secret.attributes,
                image_uri: secret.image_uri,
            },
        );
    }

    keep
}

// --- Remote signals: best-effort handshake nudges (never load-bearing) --------

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum RemoteSignal {
    /// Seller -> buyer: "you have an offer". Buyer should look up the commitment
    /// (also discoverable via the AgentToInboundCommitment link) and decide.
    TransferOffered { commitment: ActionHash },
    /// Buyer -> seller: "I accepted". Seller may now drop their stale ownership
    /// index link.
    TransferAccepted { commitment: ActionHash },
    /// Minter -> current owner: "I revealed this NFT". Owner's UI should refresh.
    NftRevealed { nft: ActionHash },
    /// Minter -> delegate at mint: "you may reveal this NFT after reveal_by".
    DelegatedReveal {
        nft: ActionHash,
        agreement: ActionHash,
    },
    /// Buyer -> seller: "I'm interested in your listing". Best effort; the
    /// interest itself is already durable via `ListingToInterested`.
    ListingInterest {
        listing: ActionHash,
        nft: ActionHash,
        buyer: AgentPubKey,
    },
}

#[hdk_extern]
pub fn recv_remote_signal(signal: RemoteSignal) -> ExternResult<()> {
    emit_signal(signal)
}

// --- Local UI signals (post_commit) -----------------------------------------
// Kept in the shape the scaffolder expects so future `hc scaffold` runs extend it.

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Signal {
    LinkCreated {
        action: SignedActionHashed,
        link_type: LinkTypes,
    },
    LinkDeleted {
        action: SignedActionHashed,
        create_link_action: SignedActionHashed,
        link_type: LinkTypes,
    },
    EntryCreated {
        action: SignedActionHashed,
        app_entry: EntryTypes,
    },
    EntryUpdated {
        action: SignedActionHashed,
        app_entry: EntryTypes,
        original_app_entry: EntryTypes,
    },
    EntryDeleted {
        action: SignedActionHashed,
        original_app_entry: EntryTypes,
    },
}

#[hdk_extern(infallible)]
pub fn post_commit(committed_actions: Vec<SignedActionHashed>) {
    for action in committed_actions {
        if let Err(err) = signal_action(action) {
            error!("Error signaling new action: {:?}", err);
        }
    }
}

fn signal_action(action: SignedActionHashed) -> ExternResult<()> {
    match &action.hashed.content.clone().data {
        ActionData::CreateLink(create_link) => {
            if let Ok(Some(link_type)) =
                LinkTypes::from_type(create_link.zome_index, create_link.link_type)
            {
                emit_signal(Signal::LinkCreated { action, link_type })?;
            }
            Ok(())
        }
        ActionData::DeleteLink(delete_link) => {
            let record = get(delete_link.link_add_address.clone(), GetOptions::default())?.ok_or(
                wasm_error!(WasmErrorInner::Guest(
                    "Failed to fetch CreateLink action".to_string()
                )),
            )?;
            match &record.action().data {
                ActionData::CreateLink(create_link) => {
                    if let Ok(Some(link_type)) =
                        LinkTypes::from_type(create_link.zome_index, create_link.link_type)
                    {
                        emit_signal(Signal::LinkDeleted {
                            action,
                            link_type,
                            create_link_action: record.signed_action.clone(),
                        })?;
                    }
                    Ok(())
                }
                _ => Err(wasm_error!(WasmErrorInner::Guest(
                    "Create Link should exist".to_string()
                ))),
            }
        }
        ActionData::Create(_create) => {
            if let Ok(Some(app_entry)) = get_entry_for_action(&action.hashed.hash) {
                emit_signal(Signal::EntryCreated { action, app_entry })?;
            }
            Ok(())
        }
        ActionData::Update(update) => {
            if let Ok(Some(app_entry)) = get_entry_for_action(&action.hashed.hash) {
                if let Ok(Some(original_app_entry)) =
                    get_entry_for_action(&update.original_action_address)
                {
                    emit_signal(Signal::EntryUpdated {
                        action,
                        app_entry,
                        original_app_entry,
                    })?;
                }
            }
            Ok(())
        }
        ActionData::Delete(delete) => {
            if let Ok(Some(original_app_entry)) = get_entry_for_action(&delete.deletes_address) {
                emit_signal(Signal::EntryDeleted {
                    action,
                    original_app_entry,
                })?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn get_entry_for_action(action_hash: &ActionHash) -> ExternResult<Option<EntryTypes>> {
    let record = match get_details(action_hash.clone(), GetOptions::default())? {
        Some(Details::Record(record_details)) => record_details.record,
        _ => return Ok(None),
    };
    let entry = match record.entry().as_option() {
        Some(entry) => entry,
        None => return Ok(None),
    };
    let (zome_index, entry_index) = match record.action().entry_type() {
        Some(EntryType::App(AppEntryDef {
            zome_index,
            entry_index,
            ..
        })) => (zome_index, entry_index),
        _ => return Ok(None),
    };
    EntryTypes::deserialize_from_type(*zome_index, *entry_index, entry)
}
