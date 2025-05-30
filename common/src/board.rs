use std::ops::Not;

use serde::{Deserialize, Serialize};

use crate::{
	piece::{Piece, PieceType},
	vec2::Vec2,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
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

#[derive(Serialize, Clone, Debug, Deserialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase")]
pub enum Floor {
	Light,
	Dark,
}

#[derive(Serialize, Clone, Debug, Deserialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Tile {
	pub floor: Floor,
	pub piece: Option<Piece>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Move {
	pub move_type: MoveType,
	pub from: Vec2,
	pub to: Vec2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
	crate = "rocket::serde",
	rename_all = "camelCase",
	rename_all_fields = "camelCase",
	tag = "type"
)]
pub enum MoveType {
	JumpingMove,
	SlidingMove,
	EnPassant,
	Promotion { into: PieceType },
	TurnEnd,
	GameOver { winner: Player },
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Board {
	pub id: String,
	pub turn: Player,
	pub white_player: String,
	pub black_player: String,
	pub board: [[Tile; 8]; 8],
	pub kings: [Vec2; 2],
	pub move_pieces: Vec<Vec2>,
	pub moves: Vec<Vec<Move>>,
}

// movegen
impl Board {
	pub fn generate_moves(&mut self, deep: bool) {
		self.move_pieces = Vec::new();
		self.moves = Vec::new();
		for y in 0..8 {
			for x in 0..8 {
				let p = Vec2(x, y);
				if let Some(piece) = &self.get_tile(p).piece {
					let moves = piece.generate_moves(self, p, deep);
					if moves.len() > 0 {
						self.moves.push(moves);
						self.move_pieces.push(p);
					}
				}
			}
		}
	}
	pub fn about_to_win(&mut self) -> bool {
		self.generate_moves(false);
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Turn {
	pub game_id: String,
	pub piece_idx: usize,
	pub move_idx: usize,
}

// do moves
impl Board {
	pub fn evaluate_turn(&mut self, turn: &Turn) -> Option<Vec<Move>> {
		let in_move = self.moves.get(turn.piece_idx)?.get(turn.move_idx)?.clone();
		let mut output_moves = vec![in_move.clone()];
		match in_move.move_type {
			MoveType::EnPassant => output_moves.insert(
				0,
				Move {
					move_type: MoveType::JumpingMove,
					from: Vec2(in_move.to.0, in_move.from.1),
					to: in_move.to,
				},
			),
			_ => {}
		}
		output_moves.push(Move {
			move_type: MoveType::TurnEnd,
			from: Vec2(-1, -1),
			to: Vec2(-1, -1),
		});
		for move_ in output_moves.iter() {
			self.do_move(move_);
		}
		return Some(output_moves);
	}
	pub fn do_move(&mut self, mov: &Move) -> () {
		if let MoveType::TurnEnd = mov.move_type {
			self.post_turn();
			return;
		}
		let start = mov.from;
		let end = mov.to;
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
			Some(Piece {
				piece_type: PieceType::King,
				owner,
				..
			}) => {
				self.kings[match owner {
					Player::White => 0,
					Player::Black => 1,
				}] = end;
			}
			_ => {}
		}
		if let Some(ref mut piece) = piece {
			piece.has_moved = true;
		}
		match &mov.move_type {
			MoveType::Promotion { into } => {
				if let Some(ref mut piece) = piece {
					piece.piece_type = into.clone();
				}
			}
			_ => {}
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
	pub fn get_player_id(&self) -> &str {
		match self.turn {
			Player::White => &self.white_player,
			Player::Black => &self.black_player,
		}
	}
	fn find_king_position(board: &[[Tile; 8]; 8], player: Player) -> Vec2 {
		board
			.iter()
			.enumerate()
			.find_map(|(y, row)| {
				let x = row.iter().position(|tile| match tile.piece {
					Some(Piece {
						piece_type: PieceType::King,
						owner,
						..
					}) => owner == player,
					_ => false,
				});
				x.map(|x| Vec2(x as i8, y as i8))
			})
			.unwrap()
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
	pub fn new(white_player: String, black_player: String, game_id: String) -> Self {
		let board = (0..8)
			.into_iter()
			.map(|i| {
				(0..8)
					.into_iter()
					.map(|j| Tile {
						floor: if (i + j) % 2 == 0 {
							Floor::Light
						} else {
							Floor::Dark
						},
						piece: DEFAULT_BOARD[i][j].clone(),
					})
					.collect::<Vec<Tile>>()
					.try_into()
					.unwrap()
			})
			.collect::<Vec<[Tile; 8]>>()
			.try_into()
			.unwrap();
		let mut board = Self {
			turn: Player::White,
			white_player,
			black_player,
			move_pieces: Default::default(),
			moves: Default::default(),
			kings: [
				Self::find_king_position(&board, Player::White),
				Self::find_king_position(&board, Player::Black),
			],
			id: game_id,
			board,
		};
		board.generate_moves(true);
		board
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct GameStart {
	pub white_player: String,
	pub black_player: String,
	pub game_id: String,
}
