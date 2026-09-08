use serde::Deserialize;
use serde::de::{Deserializer, Error, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

pub fn map<'de, D, K, T>(deserializer: D) -> Result<BTreeMap<K, T>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    T: Deserialize<'de>,
{
    deserializer.deserialize_map(Unique(PhantomData))
}

struct Unique<K, T>(PhantomData<(K, T)>);
impl<'de, K: Deserialize<'de> + Ord, T: Deserialize<'de>> Visitor<'de> for Unique<K, T> {
    type Value = BTreeMap<K, T>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("object with unique keys")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
        let mut values: BTreeMap<K, T> = BTreeMap::new();
        while let Some((key, value)) = access.next_entry::<K, T>()? {
            if values.insert(key, value).is_some() {
                return Err(M::Error::custom("duplicate field"));
            }
        }
        Ok(values)
    }
}
