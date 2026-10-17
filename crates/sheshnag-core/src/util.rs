use sha2::{Digest, Sha256};
use std::path::Path;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// First 16 hex characters of the SHA-256, the fingerprint format the baseline files use.
pub fn hash16(bytes: &[u8]) -> String {
    sha256_hex(bytes)[..16].to_owned()
}

pub fn file_hash16(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| hash16(&b))
}

/// Local time as `YYYY-MM-DD HH:MM:SS`, the log format.
pub fn stamp() -> String {
    chrono::Local::now().format("%F %T").to_string()
}

pub fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
