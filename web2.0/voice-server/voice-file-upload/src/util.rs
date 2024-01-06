use axum::http::Uri;
use serde::{Deserialize, Deserializer};

pub fn deserialize_uri<'de, D>(deserializer: D) -> Result<Uri, D::Error>
where
	D: Deserializer<'de>,
{
	String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
}
