pub const HASH_ALGORITHM: &str = "fnv1a64";

pub fn hash_hex(input: &[u8]) -> String {
    let mut value = 0xcbf29ce484222325_u64;
    for item in input.iter().copied() {
        value = (value ^ u64::from(item)).wrapping_mul(0x100000001b3);
    }
    format!("{value:016x}")
}
