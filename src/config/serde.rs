use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};

pub trait Named {
    fn name(&self) -> String;
    fn set_name(&mut self, name: String);
}

#[derive(Debug, Clone)]
pub struct NamedMap<T>(HashMap<String, T>);

impl<T> NamedMap<T> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T> Default for NamedMap<T> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<T> NamedMap<T> {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl<T> Deref for NamedMap<T> {
    type Target = HashMap<String, T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for NamedMap<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromIterator<T> for NamedMap<T>
where
    T: Named,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        Self(iter.into_iter().map(|x| (x.name(), x)).collect())
    }
}

impl<T> Serialize for NamedMap<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for NamedMap<T>
where
    T: Deserialize<'de> + Named,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut map = HashMap::<String, T>::deserialize(deserializer)?;

        for (key, value) in map.iter_mut() {
            value.set_name(key.clone());
        }

        Ok(NamedMap(map))
    }
}

#[derive(Debug, Clone)]
pub enum KnownOrCustom<A, B> {
    Known(A),
    Custom(B),
}

struct KnownOrCustomVisitor<A, B>(PhantomData<(A, B)>);

impl<'de, A, B> Visitor<'de> for KnownOrCustomVisitor<A, B>
where
    A: Deserialize<'de>,
    B: Deserialize<'de>,
{
    type Value = KnownOrCustom<A, B>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string or a map")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        A::deserialize(de::value::StrDeserializer::<E>::new(v)).map(KnownOrCustom::Known)
    }

    fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        // Deserialize as CodegenPluginConfig (a struct/map)
        B::deserialize(de::value::MapAccessDeserializer::new(map)).map(KnownOrCustom::Custom)
    }
}

impl<'de, A, B> Deserialize<'de> for KnownOrCustom<A, B>
where
    A: Deserialize<'de>,
    B: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(KnownOrCustomVisitor(PhantomData))
    }
}

impl<A, B> Serialize for KnownOrCustom<A, B>
where
    A: Serialize,
    B: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            KnownOrCustom::Known(a) => a.serialize(serializer),
            KnownOrCustom::Custom(b) => b.serialize(serializer),
        }
    }
}
