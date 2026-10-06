//! Shared canonical-encoding helpers for the G3 hashes
//! (inventory leaf/digest, assignment seed, generation seed).
//!
//! Every G3 preimage has the shape `put(tag) ‖ u32_le(version) ‖ fields`, where
//! `put(x) = u32_le(len(x)) ‖ x`, integers are little-endian, an `ActionHash`
//! is its raw 39 bytes, and the hash is blake2b-256 (`commit_hash`). The G1
//! category roll in `generation.rs` predates this and is frozen by its golden
//! vectors, so it deliberately does NOT use this form.

use hdi::prelude::*;

/// `u32_le(len) ‖ bytes`.
pub fn put_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

pub fn put_u32(buf: &mut Vec<u8>, n: u32) {
    buf.extend_from_slice(&n.to_le_bytes());
}

/// Raw 39-byte representation of an `ActionHash`.
pub fn put_action_hash(buf: &mut Vec<u8>, h: &ActionHash) {
    buf.extend_from_slice(h.get_raw_39());
}

/// Start a G3 preimage: `put(tag) ‖ u32_le(version)`.
pub fn preimage(tag: &str, version: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    put_lp(&mut buf, tag.as_bytes());
    put_u32(&mut buf, version);
    buf
}

/// Decode exactly 64 lowercase hex characters into 32 bytes.
pub fn parse_hex32(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("expected 64 hex characters, got {}", s.len()));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let d = |c: u8| match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Err("uppercase hex is not allowed".to_string()),
            _ => Err("non-hex character".to_string()),
        };
        out[i] = (d(chunk[0])? << 4) | d(chunk[1])?;
    }
    Ok(out)
}
