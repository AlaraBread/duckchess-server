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

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Game {
	pub players: Vec<u64>,
	pub board: [[Tile; 8]; 8],
}

impl Game {
	pub fn join(&mut self, player_id: u64) -> bool {
		if self.players.contains(&player_id) {
			return false;
		} else {
			self.players.push(player_id);
			return true;
		}
	}
	pub fn has_player(&self, player_id: u64) -> bool {
		return self.players.contains(&player_id);
	}
}

impl Default for Game {
	fn default() -> Self {
		Self {
			players: Default::default(),
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
