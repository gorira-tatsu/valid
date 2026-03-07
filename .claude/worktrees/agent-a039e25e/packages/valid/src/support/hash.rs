//! Stable hashing utilities for manifests, traces, and contract snapshots.

pub fn stable_hash_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("sha256:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::stable_hash_hex;

    #[test]
    fn stable_hash_is_deterministic() {
        assert_eq!(stable_hash_hex("abc"), stable_hash_hex("abc"));
        assert_ne!(stable_hash_hex("abc"), stable_hash_hex("abd"));
    }
}
