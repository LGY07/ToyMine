use tracing::error;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Sha256Digest(pub [u8; 32]);

impl From<&str> for Sha256Digest {
    fn from(value: &str) -> Self {
        let hex = hex::decode(value.trim()).unwrap_or_else(|e| {
            error!("Sha256 digest format error {e}");
            [0u8; 32].to_vec()
        });
        let hash = <[u8; 32]>::try_from(hex).unwrap_or_else(|e| {
            error!("Sha256 digest format error {e:?}");
            [0u8; 32]
        });
        Sha256Digest(hash)
    }
}
impl From<String> for Sha256Digest {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}
impl From<[u8; 32]> for Sha256Digest {
    fn from(value: [u8; 32]) -> Self {
        Sha256Digest(value)
    }
}
