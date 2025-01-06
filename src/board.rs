use std::ops::Not;

use serde::Serialize;

use crate::{
	piece::{Piece, PieceType},
	play::PlayResponse,
	vec2::Vec2,
};

#[derive(Debug, Clone, Copy, Serialize, Eq, PartialEq)]
#[serde(crate = "rocket::serde", rename_all = "camelCase")]
pub enum Player {
	White,
	Black,
}

impl Not for Player {
	type Output = Player;

	fn not(self) -> Self::Output {
		match self {
			Player::White => Player::Black,
			Player::Black => Player::White,
		}
	}
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase")]
pub enum Floor {
	Light,
	Dark,
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Tile {
	pub floor: Floor,
	pub piece: Option<Piece>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Move {
	pub move_type: MoveType,
	pub from: Vec2,
	pub to: Vec2,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum MoveType {
	JumpingMove,
	SlidingMove,
}

impl Move {
	pub fn would_cause_lose(&self, board: &Board) -> bool {
		// probably a more efficient way to do this
		let mut board = board.clone();
		board.do_move(self);
		board.post_turn();
		return board.about_to_win();
	}
}

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Board {
	pub turn: Player,
	pub white_player: u64,
	pub black_player: u64,
	pub kings: [Vec2; 2],
	pub board: [[Tile; 8]; 8],
	pub move_pieces: Vec<Vec2>,
	pub moves: Vec<Vec<Move>>,
}

// movegen
impl Board {
	pub fn generate_moves(&mut self) {
		self.move_pieces = Vec::new();
		self.moves = Vec::new();
		for y in 0..8 {
			for x in 0..8 {
				let p = Vec2(x, y);
				if let Some(piece) = &self.get_tile(p).piece {
					self.moves.push(piece.generate_moves(self, p));
					self.move_pieces.push(p);
				}
			}
		}
	}
	pub fn turn_message(&self) -> PlayResponse {
		PlayResponse::TurnStart {
			turn: self.get_player_id(),
			move_pieces: self.move_pieces.clone(),
			moves: self
				.moves
				.iter()
				.map(|moves| moves.iter().map(|m| m.to).collect())
				.collect(),
		}
	}
	pub fn about_to_win(&mut self) -> bool {
		self.generate_moves();
		self.moves
			.iter()
			.find(|moves| {
				moves
					.iter()
					.find(|m| m.to == self.get_king_position(!self.turn))
					.is_some()
			})
			.is_some()
	}
}

// do moves
impl Board {
	pub fn execute_move(&mut self, piece_idx: usize, move_idx: usize) -> Option<PlayResponse> {
		let start = *self.move_pieces.get(piece_idx)?;
		let mov = self.moves.get(piece_idx)?.get(move_idx)?.clone();
		self.do_move(&mov);
		self.post_turn();
		return Some(PlayResponse::Move {
			move_type: mov.move_type,
			from: start,
			to: mov.to,
		});
	}
	fn do_move(&mut self, mov: &Move) {
		let start = mov.from;
		let end = mov.to;
		if start == end {
			return;
		}
		let mut piece = self.get_tile(start).piece.clone();
		match &mut piece {
			Some(Piece {
				piece_type: PieceType::Pawn {
					turns_since_double_advance,
				},
				..
			}) => {
				if (start.1 - end.1).abs() > 1 {
					*turns_since_double_advance = Some(0);
				}
			}
			_ => {}
		}
		if let Some(ref mut piece) = piece {
			piece.has_moved = true;
		}
		self.get_tile_mut(end).piece = piece;
		if start != end {
			self.get_tile_mut(start).piece = Default::default();
		}
	}
	fn post_turn(&mut self) {
		for y in 0..8 {
			for x in 0..8 {
				if let Some(piece) = &mut self.get_tile_mut(Vec2(x, y)).piece {
					piece.post_turn();
				}
			}
		}
		self.turn = match self.turn {
			Player::White => Player::Black,
			Player::Black => Player::White,
		};
	}
}

impl Board {
	pub fn get_player_id(&self) -> u64 {
		match self.turn {
			Player::White => self.white_player,
			Player::Black => self.black_player,
		}
	}
	pub fn get_king_position(&self, player: Player) -> Vec2 {
		self.kings[match player {
			Player::White => 0,
			Player::Black => 1,
		}]
	}
	pub fn get_tile(&self, pos: Vec2) -> &Tile {
		&self.board[pos.1 as usize][pos.0 as usize]
	}
	pub fn get_tile_mut(&mut self, pos: Vec2) -> &mut Tile {
		&mut self.board[pos.1 as usize][pos.0 as usize]
	}
	pub fn new(white_player: u64, black_player: u64) -> Self {
		Self {
			turn: Player::White,
			kings: [Vec2(0, 0), Vec2(1, 3)],
			white_player,
			black_player,
			move_pieces: Default::default(),
			moves: Default::default(),
			board: (0..8)
				.into_iter()
				.map(|i| {
					(0..8)
						.into_iter()
						.map(|j| {
							let p = DEFAULT_BOARD[i][j].clone();
							if (i + j) % 2 == 0 {
								Tile {
									floor: Floor::Light,
									piece: p,
								}
							} else {
								Tile {
									floor: Floor::Dark,
									piece: p,
								}
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

// put the client in charge of this at some point
const DEFAULT_BOARD: [[Option<Piece>; 8]; 8] = [
	[
		Some(Piece {
			piece_type: PieceType::Castle,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Knight,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Bishop,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Queen,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::King,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Bishop,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Knight,
			owner: Player::Black,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Castle,
			owner: Player::Black,
			has_moved: false,
		}),
	],
	[const {
		Some(Piece {
			piece_type: PieceType::Pawn {
				turns_since_double_advance: None,
			},
			owner: Player::Black,
			has_moved: false,
		})
	}; 8],
	[const { None }; 8],
	[const { None }; 8],
	[const { None }; 8],
	[const { None }; 8],
	[const {
		Some(Piece {
			piece_type: PieceType::Pawn {
				turns_since_double_advance: None,
			},
			owner: Player::White,
			has_moved: false,
		})
	}; 8],
	[
		Some(Piece {
			piece_type: PieceType::Castle,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Knight,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Bishop,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Queen,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::King,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Bishop,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Knight,
			owner: Player::White,
			has_moved: false,
		}),
		Some(Piece {
			piece_type: PieceType::Castle,
			owner: Player::White,
			has_moved: false,
		}),
	],
];
