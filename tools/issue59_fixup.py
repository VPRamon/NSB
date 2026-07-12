#!/usr/bin/env python3
"""Apply compiler-driven fixes after the issue #59 codemod."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "crates/nsb-data-tools/src/checksum_io.rs"
text = path.read_text(encoding="utf-8")
text = text.replace(
    "use md5::Md5;\n",
    "use md5::{Digest as Md5Digest, Md5};\n",
)
text = text.replace(
    "use sha2::{Digest, Sha256};\nuse siderust::checksum::to_hex;\n",
    "use sha2::{Digest as Sha256Digest, Sha256};\n",
)
start = text.index("fn digest_file<D: Digest + Default>")
end = text.index("/// Compute an algorithm-qualified streaming checksum.", start)
replacement = r'''fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn sha256_digest_file(path: &Path) -> Result<[u8; 32]> {
    let file = File::open(path)
        .with_context(|| format!("failed to open {} for SHA-256", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_LEN, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_LEN];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for SHA-256", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn md5_digest_file(path: &Path) -> Result<[u8; 16]> {
    let file = File::open(path)
        .with_context(|| format!("failed to open {} for MD5", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_LEN, file);
    let mut hasher = Md5::new();
    let mut buffer = vec![0_u8; BUFFER_LEN];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for MD5", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

'''
text = text[:start] + replacement + text[end:]
old = r'''pub fn checksum_file(path: &Path, algorithm: ChecksumAlgorithm) -> Result<Checksum> {
    let bytes = match algorithm {
        ChecksumAlgorithm::Md5 => digest_file::<Md5>(path, "MD5")?,
        ChecksumAlgorithm::Sha256 => digest_file::<Sha256>(path, "SHA-256")?,
    };
    Checksum::new(algorithm, to_hex(&bytes))
}
'''
new = r'''pub fn checksum_file(path: &Path, algorithm: ChecksumAlgorithm) -> Result<Checksum> {
    let hex = match algorithm {
        ChecksumAlgorithm::Md5 => encode_hex(&md5_digest_file(path)?),
        ChecksumAlgorithm::Sha256 => encode_hex(&sha256_digest_file(path)?),
    };
    Checksum::new(algorithm, hex)
}
'''
if old not in text:
    raise RuntimeError("checksum_file template changed unexpectedly")
text = text.replace(old, new)
path.write_text(text, encoding="utf-8")
