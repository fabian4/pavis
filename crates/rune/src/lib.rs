use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct ProxyConfig {
    pub listeners: Vec<Listener>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Listener {
    pub name: String,
    pub address: String,
    pub routes: Vec<Route>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Route {
    pub path: String,
    pub upstream_address: String,
    pub host_header: Option<String>,
}
