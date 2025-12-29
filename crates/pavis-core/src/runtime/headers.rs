use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct HeaderOperations {
    // Maps of HeaderName -> HeaderValue
    pub add: Vec<(String, String)>,
    pub remove: Vec<String>,
}
