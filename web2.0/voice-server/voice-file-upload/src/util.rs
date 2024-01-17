use axum::http::Uri;
use serde::{Deserialize, Deserializer};

pub fn deserialize_uri<'de, D>(deserializer: D) -> Result<Uri, D::Error>
where
	D: Deserializer<'de>,
{
	String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
}

pub trait BooleanOption: Into<bool> + Sized {
	/// utility function to interop booleans and options
	/// equivalent to `(self).then_some( () )`.
	#[inline]
	fn option(self) -> Option<()> {
		let b: bool = self.into();
		if b {
			Some(())
		} else {
			None
		}
	}
}

impl BooleanOption for bool {}
