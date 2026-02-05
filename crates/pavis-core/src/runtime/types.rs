use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

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
#[rkyv(compare(PartialEq))]
pub struct Duration(pub NonZeroU32);

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
#[rkyv(compare(PartialEq))]
/// Consecutive error threshold for outlier detection.
pub struct ConsecutiveErrors(pub NonZeroU32);

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
#[rkyv(compare(PartialEq))]
/// Max in-flight requests allowed for an upstream.
pub struct MaxConnections(pub NonZeroU32);

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
#[rkyv(compare(PartialEq))]
/// Max pending requests allowed for an upstream.
pub struct MaxPendingRequests(pub NonZeroU32);

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
#[rkyv(compare(PartialEq))]
#[repr(u8)]
#[non_exhaustive]
pub enum Timeout {
    Disabled,
    Enabled(Duration),
}

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
#[rkyv(compare(PartialEq))]
#[repr(u8)]
#[non_exhaustive]
pub enum ConnectTimeout {
    Disabled,
    Enabled(Duration),
}

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
#[rkyv(compare(PartialEq))]
#[repr(u8)]
#[non_exhaustive]
pub enum IdleTimeout {
    Disabled,
    Enabled(Duration),
}

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
#[rkyv(compare(PartialEq))]
#[repr(u8)]
#[non_exhaustive]
pub enum TryTimeout {
    Inherit,
    Disabled,
    Enabled(Duration),
}

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct Hostname(pub String);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct Host(pub String);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct Path(pub String);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[rkyv(compare(PartialEq))]
pub struct SpiffeId(pub String);

impl SpiffeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SpiffeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<SpiffeId> for String {
    fn from(value: SpiffeId) -> Self {
        value.0
    }
}

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct ServiceName(pub String);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct HeaderName(pub String);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct HeaderValue(pub String);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct UpstreamName(pub String);

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
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct UpstreamId(pub NonZeroU16);

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct ListenerName(pub String);

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
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct Port(pub NonZeroU16);

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
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct Weight(pub NonZeroU16);

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
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct ConfigVersion(pub NonZeroU64);

impl ConfigVersion {
    pub fn get(&self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for ConfigVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

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
#[rkyv(compare(PartialEq))]
pub struct SampleRate(pub u32);

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

    #[test]
    fn test_spiffe_id() {
        let id = SpiffeId::from("spiffe://example.org/service".to_string());
        assert_eq!(id.as_str(), "spiffe://example.org/service");
        let s: String = id.into();
        assert_eq!(s, "spiffe://example.org/service");
    }

    #[test]
    fn test_config_version() {
        let v = ConfigVersion(NonZeroU64::new(42).unwrap());
        assert_eq!(v.get(), 42);
        assert_eq!(format!("{}", v), "42");
    }

    #[test]
    fn test_basic_types() {
        let _ = Duration(NonZeroU32::new(1).unwrap());
        let _ = ConsecutiveErrors(NonZeroU32::new(1).unwrap());
        let _ = MaxConnections(NonZeroU32::new(1).unwrap());
        let _ = MaxPendingRequests(NonZeroU32::new(1).unwrap());
        let _ = Hostname("host".to_string());
        let _ = Host("host".to_string());
        let _ = Path("/".to_string());
        let _ = ServiceName("svc".to_string());
        let _ = HeaderName("h".to_string());
        let _ = HeaderValue("v".to_string());
        let _ = UpstreamName("u".to_string());
        let _ = UpstreamId(NonZeroU16::new(1).unwrap());
        let _ = ListenerName("l".to_string());
        let _ = Port(NonZeroU16::new(80).unwrap());
        let _ = Weight(NonZeroU16::new(1).unwrap());
        let _ = SampleRate(100);
    }

    #[test]
    fn test_timeouts() {
        let d = Duration(NonZeroU32::new(100).unwrap());

        let t = Timeout::Enabled(d);
        if let Timeout::Enabled(inner) = t {
            assert_eq!(inner.0.get(), 100);
        }
        let _ = Timeout::Disabled;

        let ct = ConnectTimeout::Enabled(d);
        if let ConnectTimeout::Enabled(inner) = ct {
            assert_eq!(inner.0.get(), 100);
        }
        let _ = ConnectTimeout::Disabled;

        let it = IdleTimeout::Enabled(d);
        if let IdleTimeout::Enabled(inner) = it {
            assert_eq!(inner.0.get(), 100);
        }
        let _ = IdleTimeout::Disabled;

        let tt = TryTimeout::Enabled(d);
        if let TryTimeout::Enabled(inner) = tt {
            assert_eq!(inner.0.get(), 100);
        }
        let _ = TryTimeout::Inherit;
        let _ = TryTimeout::Disabled;
    }
}
