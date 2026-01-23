//! Read timeout type for P2 retry/timeout implementation

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::Duration;

/// Read timeout for response headers/body (per attempt)
#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum ReadTimeout {
    Disabled,
    Enabled(Duration),
}

impl Default for ReadTimeout {
    fn default() -> Self {
        // Default: 30 seconds
        Self::Enabled(Duration(std::num::NonZeroU32::new(30000).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let timeout = ReadTimeout::default();
        match timeout {
            ReadTimeout::Enabled(duration) => {
                assert_eq!(duration.0.get(), 30000);
            }
            _ => panic!("Expected ReadTimeout::Enabled"),
        }
    }
}
