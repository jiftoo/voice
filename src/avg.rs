use std::{fmt::Display, ops::Deref, time::Duration};

pub struct SlidingAverage {
	items: Vec<Duration>,
	size: usize,
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DisplayDuration(pub Duration);

impl Deref for DisplayDuration {
	type Target = Duration;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl Display for DisplayDuration {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let secs = self.0.as_secs();
		// display the most appropriate unit (seconds, minutes, hours or days)
		// hours also show minutes, minutes also show seconds
		if secs < 60 {
			write!(f, "{}s", secs)
		} else if secs < 60 * 60 {
			write!(f, "{}m {}s", secs / 60, secs % 60)
		} else if secs < 60 * 60 * 24 {
			write!(f, "{}h {}m {}s", secs / (60 * 60), (secs / 60) % 60, secs % 60)
		} else {
			write!(
				f,
				"{}d {}h {}m {}s",
				secs / (60 * 60 * 24),
				(secs / (60 * 60)) % 24,
				(secs / 60) % 60,
				secs % 60
			)
		}
	}
}

impl SlidingAverage {
	pub fn new(size: usize) -> Self {
		Self { items: Vec::with_capacity(size), size }
	}

	pub fn push(&mut self, item: Duration) -> Duration {
		self.items.push(item);
		if self.items.len() > self.size {
			self.items.remove(0);
		}
		self.average()
	}

	pub fn average(&self) -> Duration {
		self.items.iter().sum::<Duration>() / self.items.len() as u32
	}
}
