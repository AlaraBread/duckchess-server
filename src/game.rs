use std::{collections::HashMap, sync::atomic::AtomicU64};

use rocket::serde;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PieceType {
	King { has_castled: bool },
	Queen,
	Castle,
	Bishop,
	Knight,
	Pawn { has_moved: bool },
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum FloorType {
	Light,
	Dark,
	Ice,
	Water,
	Fire,
	Wall,
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Piece {
	piece_type: PieceType,
	owner: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Tile {
	floor: FloorType,
	piece: Option<Piece>,
}

impl Tile {
	pub fn new(floor: FloorType) -> Self {
		return Self { floor, piece: None };
	}
}

#[derive(Debug)]
pub struct Game {
	pub listening_players: HashMap<u64, i32>,
	pub players: Vec<u64>,
	pub board: [[Tile; 8]; 8],
	pub cleanup_counter: AtomicU64,
}

impl Game {
	pub fn join(&mut self, player_id: u64) -> bool {
		if self.players.contains(&player_id) || self.players.len() >= 2 {
			return false;
		} else {
			self.players.push(player_id);
			return true;
		}
	}
	pub fn update_listeners(&mut self, player_id: u64, change: i32) -> i32 {
		let listeners = self.listening_players.entry(player_id).or_insert(0);
		*listeners += change;
		*listeners
	}
	pub fn get_listen_count(&mut self, player_id: u64) -> i32 {
		*self.listening_players.entry(player_id).or_insert(0)
	}
	pub fn get_total_listeners(&mut self) -> i32 {
		self.players.iter().fold(0, |acc, player_id| {
			acc + if *self.listening_players.entry(*player_id).or_insert(0) > 0 {
				1
			} else {
				0
			}
		})
	}
	pub fn has_player(&self, player_id: u64) -> bool {
		return self.players.contains(&player_id);
	}
}

impl Default for Game {
	fn default() -> Self {
		Self {
			players: Default::default(),
			listening_players: Default::default(),
			cleanup_counter: AtomicU64::new(0),
			board: (0..8)
				.into_iter()
				.map(|i| {
					(0..8)
						.into_iter()
						.map(|j| {
							if (i + j) % 2 == 0 {
								Tile::new(FloorType::Light)
							} else {
								Tile::new(FloorType::Dark)
							}
						})
						.collect::<Vec<Tile>>()
						.try_into()
						.unwrap()
				})
				.collect::<Vec<[Tile; 8]>>()
				.try_into()
				.unwrap(),
		}
	}
}
