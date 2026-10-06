//! Runs `unyt/nft_sale_settlement` through the real `rave_engine` 0.13.0.

use hdi::prelude::{ActionHash, ActionHashB64, AgentPubKey, AgentPubKeyB64};
use rave_engine::prelude::RhaiEngine;
use rave_engine::types::entries::rave::rave_output::{RAVEOutput, UnitAllocations};
use serde_json::{json, Value};
use std::path::Path;

fn script() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../unyt/nft_sale_settlement/execution_code.rhai");
    let code = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    rmp_serde::to_vec(&code).unwrap()
}

fn seller() -> String {
    AgentPubKeyB64::from(AgentPubKey::from_raw_32(vec![9; 32])).to_string()
}

fn nft_id() -> String {
    "uhCkk-nft-under-sale".to_string()
}

fn source(seed: u8) -> String {
    ActionHashB64::from(ActionHash::from_raw_32(vec![seed; 32])).to_string()
}

fn spend(amount: Value, seed: u8) -> Value {
    json!({ "data": { "amount": amount, "source": source(seed) } })
}

fn run(spends: Vec<Value>, price: Value) -> Result<RAVEOutput, String> {
    let input = json!({
        "consumed_inputs": { "buyer_spends": spends },
        "inputs": {
            "seller": { "data": seller() },
            "price": { "data": price },
            "nft_id": { "data": nft_id() },
        },
    });
    RhaiEngine::new()
        .execute(&input, script(), None)
        .map(|r| r.output)
        .map_err(|e| format!("{e:?}"))
}

fn allocations(out: &RAVEOutput) -> UnitAllocations {
    out.unyt_allocation.clone().expect("unyt_allocation")
}

#[test]
fn exact_payment_pays_the_seller_from_every_spend() {
    let out = run(
        vec![spend(json!({ "0": "6" }), 1), spend(json!({ "0": "4" }), 2)],
        json!({ "0": "10" }),
    )
    .expect("exact payment must settle");

    let allocs = allocations(&out);
    assert_eq!(allocs.len(), 1, "one allocation to the seller");
    assert_eq!(allocs[0].sources.len(), 2, "both buyer spends are sourced");
    let amounts = serde_json::to_value(&allocs[0].amounts).unwrap();
    assert_eq!(amounts, json!({ "0": "10" }));
}

#[test]
fn computed_values_name_the_nft_and_price() {
    let out = run(vec![spend(json!({ "0": "10" }), 1)], json!({ "0": "10" })).unwrap();
    let cv = out.computed_values.expect("computed_values");
    assert_eq!(cv["nft_id"], json!(nft_id()));
    assert_eq!(cv["price"], json!(10.0));
}

#[test]
fn underpayment_is_refused() {
    let err = run(vec![spend(json!({ "0": "9" }), 1)], json!({ "0": "10" })).unwrap_err();
    assert!(err.contains("exactly the agreed price"), "{err}");
}

#[test]
fn overpayment_is_refused_so_no_spend_is_burned_without_paying() {
    let err = run(vec![spend(json!({ "0": "11" }), 1)], json!({ "0": "10" })).unwrap_err();
    assert!(err.contains("exactly the agreed price"), "{err}");
}

#[test]
fn no_spends_is_refused() {
    let err = run(vec![], json!({ "0": "10" })).unwrap_err();
    assert!(err.contains("exactly the agreed price"), "{err}");
}

#[test]
fn zero_price_is_refused() {
    let err = run(vec![spend(json!({ "0": "0" }), 1)], json!({ "0": "0" })).unwrap_err();
    assert!(err.contains("greater than zero"), "{err}");
}
