use rocket::serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

use crate::Player;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct ChessClock {
	white: Timer,
	black: Timer,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(
	crate = "rocket::serde",
	rename_all_fields = "camelCase",
	rename_all = "camelCase",
	tag = "type"
)]
pub enum Timer {
	Running { end_time: u64 },
	Paused { time_remaining: u64 },
}

impl ChessClock {
	pub fn new(time_seconds: u64) -> ChessClock {
		ChessClock {
			white: Timer::new(time_seconds),
			black: Timer::new(time_seconds),
		}
	}
	pub fn player_timer(&mut self, player: Player) -> &mut Timer {
		match player {
			Player::White => &mut self.white,
			Player::Black => &mut self.black,
		}
	}
}

impl Timer {
	pub fn new(time_seconds: u64) -> Timer {
		Timer::Paused {
			time_remaining: time_seconds,
		}
	}
	pub fn start(&mut self) {
		if let Timer::Paused { time_remaining } = self {
			*self = Timer::Running {
				end_time: SystemTime::now()
					.checked_add(Duration::from_secs(*time_remaining))
					.expect("u64 time overflow")
					.duration_since(SystemTime::UNIX_EPOCH)
					.expect("system time before unix epoch")
					.as_secs(),
			};
		}
	}
	pub fn pause(&mut self) -> bool {
		match self {
			Timer::Running { end_time } => {
				match SystemTime::UNIX_EPOCH
					.checked_add(Duration::from_secs(*end_time))
					.expect("u64 time overflow")
					.duration_since(SystemTime::now())
				{
					Ok(time_remaining) => {
						*self = Timer::Paused {
							time_remaining: time_remaining.as_secs(),
						};
						true
					}
					Err(_) => false,
				}
			}
			Timer::Paused { .. } => true,
		}
	}
	pub fn has_time(&self) -> bool {
		match self {
			Timer::Running { end_time } => {
				match SystemTime::UNIX_EPOCH
					.checked_add(Duration::from_secs(*end_time))
					.expect("u64 time overflow")
					.duration_since(SystemTime::now())
				{
					Ok(_) => true,
					Err(_) => false,
				}
			}
			Timer::Paused { time_remaining } => *time_remaining > 0,
		}
	}
}
