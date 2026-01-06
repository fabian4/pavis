use crate::runtime::types::{ListenerName, Path};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::num::NonZeroU16;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Listener {
    pub name: ListenerName,
    pub address: SocketAddr,
    pub workers: WorkerCount,
    pub tls: TlsConfig,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum WorkerCount {
    Auto,
    Count(NonZeroU16),
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum TlsConfig {
    Disabled,
    Enabled {
        cert_path: Path,
        key_path: Path,
        client_auth: ClientAuth,
    },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum ClientAuth {
    Disabled,
    Optional { ca_path: Path },
    Required { ca_path: Path },
}
