/// Subject: NFT sale escrow -- atomically pays the seller once a buyer's parked payment exactly
///   matches the seller's listed price for a given NFT.
///
/// Inputs:
/// listing: consumed ParkedData submission from the "seller" role -- nft_id, unyt_index, price,
///   and the seller's payout receiver key, printed/parked when the seller lists the NFT for sale.
/// escrow_lock: consumed ParkedSpendBalance submission from the "buyer_spender" role -- the buyer's
///   payment, locked for this specific listing.
///
/// Summary:
/// This code settles the PAYMENT leg of an NFT sale only. It does not mint, own, or transfer any
/// NFT itself -- Unyt Smart Agreements have no concept of a non-fungible token, and Rhai here cannot
/// call into another DNA's zome. The actual NFT registry (uniqueness, current-owner index, transfer
/// validation) must live in a separate Holochain app; this agreement produces the auditable,
/// atomically-paid RAVE that app watches for before it finalizes a transfer. See README, section
/// "What this does NOT do," before using this in production.

// A non-aggregate consumed input (aggregate_execution: false) is delivered as ONE envelope
// `#{ data, link_hash }` -- the role's latest parked link -- not an array. This helper reads the
// payload under either delivery shape and returns () when nothing is parked.
fn latest_payload(input) {
    if type_of(input) == "map" { return input.data; }
    if type_of(input) == "array" && input.len() > 0 { return input[0].data; }
    ()
}

let listing_in = latest_payload(consumed_inputs.listing);
if type_of(listing_in) == "()" {
    throw "No seller listing has been parked yet";
}
let escrow_lock_in = latest_payload(consumed_inputs.escrow_lock);
if type_of(escrow_lock_in) == "()" {
    throw "No buyer escrow lock has been parked yet";
}

let listing = listing_in;
let escrow_lock = escrow_lock_in;

let unyt_index = listing.unyt_index;
let price = listing.price;

if !escrow_lock.amount.contains(unyt_index) {
    throw "Escrow lock does not cover unyt_index " + unyt_index + " for nft " + listing.nft_id;
}

let locked_amount = escrow_lock.amount[unyt_index];

// Exact match, both directions, using the engine's fuel-string comparator rather than a string ==
// (would wrongly reject equivalent amounts like "20" vs "20.00") or a float cast (precision risk).
// See README "Security Considerations" for why this template requires exact match, not "at least."
let locked_covers_price = is_greater_than_eq(locked_amount, price);
let price_covers_locked = is_greater_than_eq(price, locked_amount);

if !(locked_covers_price && price_covers_locked) {
    throw "Escrow lock of " + locked_amount + " does not exactly match the listed price of " +
          price + " for nft " + listing.nft_id;
}

// Resolve the buyer's identity cryptographically from the parked spend link's creator, rather than
// trusting an ExecutorProvided claim. This is the value a companion NFT zome should treat as the new
// owner once it observes this RAVE -- see README "Wiring this to an NFT registry."
let buyer_key = "";
try {
    buyer_key = get_spend_links_author(escrow_lock.source);
} catch(e) {
    throw "Could not resolve the buyer's identity from the escrow lock: " + e;
}

let amounts = #{};
amounts[unyt_index] = locked_amount;

let output = #{
    "unyt_allocation": [
        #{
            "receiver": listing.receiver,
            "amounts": amounts,
            "sources": [escrow_lock.source]
        }
    ],
    "computed_values": #{
        "nft_id": listing.nft_id,
        "unyt_index": unyt_index,
        "settled_amount": locked_amount,
        "previous_owner": listing.receiver,
        "new_owner": buyer_key,
        "fulfilled": true
    }
};

return #{ "output": output };
