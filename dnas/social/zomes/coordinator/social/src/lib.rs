pub mod collection;
pub mod comment;
pub mod follow;
pub mod like;
pub mod nft_collection_registry;

use hdk::prelude::*;

// Called the first time a zome call is made to the cell containing this zome.
#[hdk_extern]
pub fn init() -> ExternResult<InitCallbackResult> {
    // Allow any agent to deliver a "you were followed" nudge to this agent.
    let mut functions = HashSet::new();
    functions.insert((zome_info()?.name, FunctionName::from("recv_remote_signal")));
    create_cap_grant(ZomeCallCapGrant {
        tag: "recv_remote_signal".into(),
        access: CapAccess::Unrestricted,
        functions: GrantedFunctions::Listed(functions),
    })?;
    Ok(InitCallbackResult::Pass)
}

// --- Remote signals: best-effort nudges (never load-bearing) ----------------

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum RemoteSignal {
    /// Someone -> you: "you have a new follower". Best effort; the follow
    /// itself is already durable on the DHT regardless of delivery.
    Followed,
}

#[hdk_extern]
pub fn recv_remote_signal(signal: RemoteSignal) -> ExternResult<()> {
    emit_signal(signal)
}
