use crate::runtime::types::{HeaderName, HeaderValue};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum HeadersPolicy {
    Disabled,
    Enabled { rules: Headers },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Headers {
    pub set_headers: Vec<(HeaderName, HeaderValue)>,
    pub append_headers: Vec<(HeaderName, HeaderValue)>,
    pub add_headers: Vec<(HeaderName, HeaderValue)>,
    pub remove_headers: Vec<HeaderName>,
}
