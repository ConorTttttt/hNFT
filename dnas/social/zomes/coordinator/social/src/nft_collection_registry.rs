//! Phase J.3 — network-wide NFT-collection discovery. See
//! `social_integrity::nft_collection_registry` for the protocol, the
//! authorization boundary, and why this lives in `social` (the one global,
//! non-cloned cell every agent shares) rather than in any `nft` clone.

use hdk::prelude::*;
use social_integrity::*;
use std::collections::BTreeMap;

const PATH: &str = "nft-collections.all";

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterNftCollectionInput {
    /// The `nft` clone's own `DnaHash` — read by the CALLER from its local
    /// conductor state (`appInfo()`'s matching `ClonedCell.cell_id[0]`), not
    /// invented client-side.
    pub dna_hash: DnaHash,
    /// That clone's live `dna_modifiers.network_seed`, verbatim.
    pub network_seed: String,
    /// That clone's live `dna_modifiers.properties`, decoded and re-encoded
    /// as JSON, verbatim. See the integrity module doc for why this must be
    /// carried in full rather than a smaller summary.
    pub properties_json: String,
}

/// Publish (or re-publish) a discovery announcement for an `nft` collection
/// clone this agent already has installed. Authorization is exactly
/// `action.author()` on the resulting entry — see the integrity module's
/// authorization-boundary note for what that does and does not prove.
#[hdk_extern]
pub fn register_nft_collection(input: RegisterNftCollectionInput) -> ExternResult<Record> {
    let reg = NftCollectionRegistration {
        dna_hash: input.dna_hash,
        network_seed: input.network_seed,
        properties_json: input.properties_json,
    };
    let hash = create_entry(&EntryTypes::NftCollectionRegistration(reg))?;

    let me = agent_info()?.agent_initial_pubkey;
    create_link(
        me,
        hash.clone(),
        LinkTypes::AgentToNftCollectionRegistration,
        (),
    )?;
    let path = Path::from(PATH);
    create_link(
        path.path_entry_hash()?,
        hash.clone(),
        LinkTypes::PathToNftCollectionRegistration,
        (),
    )?;

    get(hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created NftCollectionRegistration not found".into()
    )))
}

/// Withdraw one of THIS agent's own registrations (author-only, enforced in
/// `validate()`). Does not affect the underlying `nft` clone in any way —
/// purely a discovery-directory operation.
#[hdk_extern]
pub fn unregister_nft_collection(registration: ActionHash) -> ExternResult<()> {
    delete_entry(registration)?;
    Ok(())
}

/// This agent's own published registrations (e.g. "already published" state
/// in the Create-Collection flow / Overview).
#[hdk_extern]
pub fn my_nft_collection_registrations(_: ()) -> ExternResult<Vec<Record>> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToNftCollectionRegistration)?,
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

/// One resolved, discovery-ready collection announcement.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NftCollectionDiscoveryView {
    pub registration: ActionHash,
    pub dna_hash: DnaHash,
    pub network_seed: String,
    pub properties_json: String,
    /// Whoever published this announcement — cryptographically real, but see
    /// the integrity module's authorization-boundary note for what it does
    /// and does not prove about "creatorship" of the underlying collection.
    pub registered_by: AgentPubKey,
    pub registered_at_micros: i64,
}

/// Every network-wide-discoverable NFT collection, deduplicated by
/// `dna_hash` (the collection's true identity — see the integrity module
/// doc). Multiple independent agents CAN register the same `dna_hash`;
/// nothing in `validate()` can prevent that (no shared counter/lock across
/// independent authors' entries, the same limitation this codebase has
/// documented for every other cross-agent aggregate case). This query
/// resolves that deterministically for every caller by keeping the
/// EARLIEST-timestamped registration per `dna_hash` — a display heuristic,
/// not a security or ownership determination.
#[hdk_extern]
pub fn discover_nft_collections(_: ()) -> ExternResult<Vec<NftCollectionDiscoveryView>> {
    let path = Path::from(PATH);
    let links = get_links(
        LinkQuery::try_new(
            path.path_entry_hash()?,
            LinkTypes::PathToNftCollectionRegistration,
        )?,
        GetStrategy::Network,
    )?;
    let get_inputs: Vec<GetInput> = links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .map(|h| GetInput::new(h.into(), GetOptions::default()))
        .collect();
    let records: Vec<Record> = HDK
        .with(|hdk| hdk.borrow().get(get_inputs))?
        .into_iter()
        .flatten()
        .collect();

    let mut by_dna: BTreeMap<DnaHash, NftCollectionDiscoveryView> = BTreeMap::new();
    for rec in records {
        let Some(reg): Option<NftCollectionRegistration> = rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        else {
            continue; // not (or no longer) an NftCollectionRegistration — skip, don't fail the whole query.
        };
        let view = NftCollectionDiscoveryView {
            registration: rec.action_address().clone(),
            dna_hash: reg.dna_hash.clone(),
            network_seed: reg.network_seed,
            properties_json: reg.properties_json,
            registered_by: rec.action().author().clone(),
            registered_at_micros: rec.action().timestamp().as_micros(),
        };
        match by_dna.get(&reg.dna_hash) {
            Some(existing) if existing.registered_at_micros <= view.registered_at_micros => {}
            _ => {
                by_dna.insert(reg.dna_hash, view);
            }
        }
    }
    let mut out: Vec<_> = by_dna.into_values().collect();
    out.sort_by_key(|v| v.registered_at_micros);
    Ok(out)
}
