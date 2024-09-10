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
