/// Subject: NFT sale settlement (exact price, single unit "0")
///
/// Inputs:
/// consumed_inputs.buyer_spends: the buyer's parked spend links (aggregate)
/// inputs.seller:   AgentPubKey paid for the NFT (Fixed on the agreement)
/// inputs.price:    unit map; index "0" is the agreed price (Fixed)
/// inputs.nft_id:   string id of the NFT being sold (Fixed)
///
/// Summary:
/// Pays exactly `price` to `seller`, drawing from every buyer spend. Throws
/// unless the spends total exactly `price`. Consumed inputs are burned even
/// when nothing is paid, so refusing before any allocation is what keeps a
/// buyer's parked spend from being lost on a mismatch.

let price_str = inputs.price.data["0"];
if is_greater_than_eq("0", price_str) {
  throw "price must be greater than zero";
}

let total = #{ "0": "0" };
let sources = [];
for s in consumed_inputs.buyer_spends {
  total = add_units(total, extract_indexes(s.data.amount, ["0"]));
  sources.push(s.data.source);
}

// Exact match in both directions.
if !is_greater_than_eq(total["0"], price_str) || !is_greater_than_eq(price_str, total["0"]) {
  throw "buyer spends must total exactly the agreed price";
}

return #{
  "output": #{
    "unyt_allocation": [
      #{
        "receiver": inputs.seller.data,
        "amounts": #{ "0": price_str },
        "sources": sources
      }
    ],
    "computed_values": #{
      "nft_id": inputs.nft_id.data,
      "price": to_num(price_str)
    }
  }
};
