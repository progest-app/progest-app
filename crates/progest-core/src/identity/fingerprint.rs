//! Content fingerprint: blake3 hash truncated to 128 bits.
//!
//! The full 256-bit blake3 hash is overkill for duplicate-detection at the
//! scale a single project sees (tens of thousands of files); 128 bits keeps
//! the on-disk `.meta` strings short and indexable while retaining a
//! collision probability well below what a project ever encounters.
//!
//! Serialized form is `blake3:<32 lowercase hex chars>`, preserving the
//! algorithm tag so a future migration to a different hash can be detected
//! and handled in `progest doctor`.

use std::fmt;
use std::io::{self, Read};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Number of bytes retained from the full blake3 digest.
pub const FINGERPRINT_BYTES: usize = 16;
/// Number of lowercase hex characters required to round-trip [`Fingerprint`].
pub const FINGERPRINT_HEX_LEN: usize = FINGERPRINT_BYTES * 2;
/// Algorithm tag that prefixes the serialized hex digest.
pub const FINGERPRINT_PREFIX: &str = "blake3:";

/// Errors returned while parsing or computing a [`Fingerprint`].
#[derive(Debug, Error)]
pub enum FingerprintError {
    #[error("fingerprint must start with `{FINGERPRINT_PREFIX}`, got `{0}`")]
    MissingPrefix(String),
    #[error("fingerprint hex must be {FINGERPRINT_HEX_LEN} characters, got {0}")]
    BadLength(usize),
    #[error("fingerprint hex contains invalid character `{0}` at position {1}")]
    BadHex(char, usize),
    #[error("I/O error while hashing: {0}")]
    Io(#[from] io::Error),
}

/// A content fingerprint derived from the first 16 bytes of a blake3 digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Fingerprint([u8; FINGERPRINT_BYTES]);

impl Fingerprint {
    /// Wrap a pre-computed 16-byte digest.
    #[must_use]
    pub fn from_bytes(bytes: [u8; FINGERPRINT_BYTES]) -> Self {
        Self(bytes)
    }

    /// Raw bytes of the truncated digest.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; FINGERPRINT_BYTES] {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(FINGERPRINT_PREFIX)?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Fingerprint {
    type Err = FingerprintError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(hex) = s.strip_prefix(FINGERPRINT_PREFIX) else {
            return Err(FingerprintError::MissingPrefix(s.to_string()));
        };
        if hex.len() != FINGERPRINT_HEX_LEN {
            return Err(FingerprintError::BadLength(hex.len()));
        }
        let mut bytes = [0u8; FINGERPRINT_BYTES];
        for (i, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
            let hi = hex_digit(pair[0], i * 2)?;
            let lo = hex_digit(pair[1], i * 2 + 1)?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
}

impl From<Fingerprint> for String {
    fn from(fp: Fingerprint) -> String {
        fp.to_string()
    }
}

impl TryFrom<String> for Fingerprint {
    type Error = FingerprintError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

fn hex_digit(byte: u8, pos: usize) -> Result<u8, FingerprintError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        other => Err(FingerprintError::BadHex(other as char, pos)),
    }
}

/// Stream `reader` through blake3 and return the truncated [`Fingerprint`].
///
/// Reads are buffered in 8 KiB chunks so the memory footprint stays constant
/// regardless of file size. Any I/O error from `reader` is surfaced via
/// [`FingerprintError::Io`].
pub fn compute_fingerprint<R: Read>(mut reader: R) -> Result<Fingerprint, FingerprintError> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize();
    let full = hash.as_bytes();
    let mut truncated = [0u8; FINGERPRINT_BYTES];
    truncated.copy_from_slice(&full[..FINGERPRINT_BYTES]);
    Ok(Fingerprint(truncated))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn display_parse_roundtrip() {
        let fp = Fingerprint::from_bytes([
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ]);
        let rendered = fp.to_string();
        assert_eq!(rendered, "blake3:00112233445566778899aabbccddeeff");
        assert_eq!(rendered.parse::<Fingerprint>().unwrap(), fp);
    }

    #[test]
    fn parse_rejects_missing_prefix() {
        let raw = "deadbeefcafef00dbaadf00d12345678";
        assert!(matches!(
            raw.parse::<Fingerprint>(),
            Err(FingerprintError::MissingPrefix(_))
        ));
    }

    #[test]
    fn parse_rejects_wrong_length() {
        assert!(matches!(
            "blake3:deadbeef".parse::<Fingerprint>(),
            Err(FingerprintError::BadLength(8))
        ));
    }

    #[test]
    fn parse_rejects_uppercase_hex() {
        // Strict lowercase keeps the on-disk representation canonical.
        assert!(matches!(
            "blake3:AABBCCDDEEFF00112233445566778899".parse::<Fingerprint>(),
            Err(FingerprintError::BadHex(_, _))
        ));
    }

    #[test]
    fn compute_matches_expected_blake3_truncated() {
        // Expected value: first 16 bytes of blake3("hello world").
        let hash = blake3::hash(b"hello world");
        let expected = {
            let mut bytes = [0u8; FINGERPRINT_BYTES];
            bytes.copy_from_slice(&hash.as_bytes()[..FINGERPRINT_BYTES]);
            Fingerprint::from_bytes(bytes)
        };

        let actual = compute_fingerprint(Cursor::new(b"hello world")).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn compute_handles_empty_input() {
        let fp = compute_fingerprint(Cursor::new(b"")).unwrap();
        // Any empty input yields the same fingerprint — just ensure it round-trips.
        let rendered = fp.to_string();
        assert!(rendered.starts_with(FINGERPRINT_PREFIX));
        assert_eq!(
            rendered.len(),
            FINGERPRINT_PREFIX.len() + FINGERPRINT_HEX_LEN
        );
    }

    #[test]
    fn streaming_produces_same_digest_as_one_shot() {
        // Data that exceeds the 8 KiB buffer exercises the loop boundary.
        let data: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        let streamed = compute_fingerprint(Cursor::new(&data)).unwrap();

        let mut hasher = blake3::Hasher::new();
        hasher.update(&data);
        let mut bytes = [0u8; FINGERPRINT_BYTES];
        bytes.copy_from_slice(&hasher.finalize().as_bytes()[..FINGERPRINT_BYTES]);
        let one_shot = Fingerprint::from_bytes(bytes);

        assert_eq!(streamed, one_shot);
    }

    #[test]
    fn serde_roundtrips_as_string() {
        let fp = Fingerprint::from_bytes([
            0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xf0, 0x0d, 0xba, 0xad, 0xf0, 0x0d, 0x12, 0x34,
            0x56, 0x78,
        ]);
        let json = serde_json::to_string(&fp).unwrap();
        assert_eq!(json, "\"blake3:deadbeefcafef00dbaadf00d12345678\"");
        let back: Fingerprint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fp);
    }

    #[test]
    fn compute_surfaces_io_errors() {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("boom"))
            }
        }
        let err = compute_fingerprint(FailingReader).unwrap_err();
        assert!(matches!(err, FingerprintError::Io(_)));
    }
}
