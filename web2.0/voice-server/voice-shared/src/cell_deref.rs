use std::{future::Future, ops::Deref};

use tokio::sync::OnceCell;

pub struct OnceCellDeref<T>(OnceCell<T>);

impl<T> OnceCellDeref<T> {
	pub const fn const_new() -> Self {
		Self(OnceCell::const_new())
	}

	pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
	where
		F: FnOnce() -> Fut,
		Fut: Future<Output = T>,
	{
		self.0.get_or_init(f).await
	}
}

impl<T> Deref for OnceCellDeref<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		self.0.get().unwrap()
	}
}
