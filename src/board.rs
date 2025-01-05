use serde::Serialize;

use crate::play::PlayResponse;

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PieceType {
	King,
	Queen,
	Castle,
	Bishop,
	Knight,
	Pawn {
		turns_since_double_advance: Option<i32>,
	},
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase")]
pub enum FloorType {
	Light,
	Dark,
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Piece {
	piece_type: PieceType,
	owner: u64,
	has_moved: bool,
}

impl Piece {
	fn generate_simple_moves(
		&self,
		offsets: &[(i8, i8)],
		limit: i8,
		pos: (i8, i8),
		move_type: MoveType,
		board: &Board,
	) -> Vec<Move> {
		let mut moves = Vec::new();
		for dir in offsets {
			let (mut x, mut y) = pos;
			x += dir.0;
			y += dir.1;
			let mut limit = limit;
			while x >= 0 && x < 8 && y >= 0 && y < 8 && limit > 0 {
				if let Some(blocking) = &board.board[y as usize][x as usize].piece {
					if blocking.owner != self.owner {
						// capture
						moves.push(Move {
							move_type,
							to: (x, y),
						});
					}
					break;
				}
				moves.push(Move {
					move_type,
					to: (x, y),
				});
				x += dir.0;
				y += dir.1;
				limit -= 1;
			}
		}
		return moves;
	}
	pub fn generate_moves(&self, board: &Board, pos: (i8, i8)) -> Vec<Move> {
		if self.owner != board.turn {
			return vec![];
		}
		return match self.piece_type {
			PieceType::King => self.generate_simple_moves(
				&[
					(0, 1),
					(0, -1),
					(1, 0),
					(-1, 0),
					(-1, -1),
					(-1, 1),
					(1, -1),
					(1, 1),
				],
				1,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Queen => self.generate_simple_moves(
				&[
					(0, 1),
					(0, -1),
					(1, 0),
					(-1, 0),
					(-1, -1),
					(-1, 1),
					(1, -1),
					(1, 1),
				],
				i8::MAX,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Castle => self.generate_simple_moves(
				&[(0, 1), (0, -1), (1, 0), (-1, 0)],
				i8::MAX,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Bishop => self.generate_simple_moves(
				&[(-1, -1), (-1, 1), (1, -1), (1, 1)],
				i8::MAX,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Knight => self.generate_simple_moves(
				&[
					(2, 1),
					(2, -1),
					(-2, 1),
					(-2, -1),
					(1, 2),
					(1, -2),
					(-1, 2),
					(-1, -2),
				],
				1,
				pos,
				MoveType::JumpingMove,
				board,
			),
			PieceType::Pawn { .. } => {
				let limit = if self.has_moved { 1 } else { 2 };
				let dir = if self.owner == board.white_player {
					(0, -1)
				} else {
					(0, 1)
				};
				// advance by 1 and 2
				let mut moves = (1..=limit)
					.map(|i| Move {
						move_type: MoveType::SlidingMove,
						to: (pos.0 + dir.0 * i, pos.1 + dir.1 * i),
					})
					.filter(|m| {
						let piece = &board.board[m.to.1 as usize][m.to.0 as usize].piece;
						if let Some(_) = piece {
							false
						} else {
							true
						}
					})
					.collect::<Vec<Move>>();
				// capture moves
				for side in [-1, 1] {
					if let Some(piece) =
						&board.board[(pos.1 + dir.1) as usize][(pos.0 + side) as usize].piece
					{
						if piece.owner != self.owner {
							moves.push(Move {
								move_type: MoveType::SlidingMove,
								to: (pos.0 + side, pos.1 + dir.1),
							});
						}
					}
				}
				// en passant captures
				for side in [-1, 1] {
					match &board.board[pos.1 as usize][(pos.0 + side) as usize].piece {
						Some(Piece {
							piece_type:
								PieceType::Pawn {
									turns_since_double_advance: Some(1),
								},
							owner,
							..
						}) => {
							if *owner != self.owner {
								moves.push(Move {
									move_type: MoveType::SlidingMove,
									to: (pos.0 + side, pos.1 + dir.1),
								})
							}
						}
						_ => {}
					}
				}
				return moves;
			}
		};
	}
	pub fn post_turn(&mut self) {
		match &mut self.piece_type {
			PieceType::Pawn {
				turns_since_double_advance: Some(turns_since_double_advance),
			} => *turns_since_double_advance += 1,
			_ => {}
		}
	}
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

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Move {
	move_type: MoveType,
	to: (i8, i8),
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum MoveType {
	JumpingMove,
	SlidingMove,
}

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Board {
	pub turn: u64,
	pub white_player: u64,
	pub black_player: u64,
	pub board: [[Tile; 8]; 8],
	pub move_pieces: Vec<(i8, i8)>,
	pub moves: Vec<Vec<Move>>,
}

// movegen
impl Board {
	pub fn generate_moves(&mut self) -> PlayResponse {
		self.move_pieces = Vec::new();
		self.moves = Vec::new();
		for x in 0..8 {
			for y in 0..8 {
				if let Some(piece) = &self.board[y as usize][x as usize].piece {
					self.move_pieces.push((x, y));
					self.moves.push(piece.generate_moves(self, (x, y)));
				}
			}
		}
		return PlayResponse::TurnStart {
			turn: self.turn,
			move_pieces: self.move_pieces.clone(),
			moves: self
				.moves
				.iter()
				.map(|moves| moves.iter().map(|m| m.to).collect())
				.collect(),
		};
	}
}

// do moves
impl Board {
	pub fn execute_move(&mut self, piece_idx: usize, move_idx: usize) -> Option<PlayResponse> {
		let start = *self.move_pieces.get(piece_idx)?;
		let mov = self.moves.get(piece_idx)?.get(move_idx)?.clone();
		let end = mov.to;
		self.do_move(start, end);
		self.post_turn();
		self.turn = if self.turn == self.white_player {
			self.black_player
		} else {
			self.white_player
		};
		return Some(PlayResponse::Move {
			move_type: mov.move_type,
			from: start,
			to: end,
		});
	}
	fn do_move(&mut self, start: (i8, i8), end: (i8, i8)) {
		if start == end {
			return;
		}
		let mut piece = self.board[start.1 as usize][start.0 as usize].piece.clone();
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
		self.board[end.1 as usize][end.0 as usize].piece = piece;
		self.board[start.1 as usize][start.0 as usize].piece = Default::default();
	}
	fn post_turn(&mut self) {
		for x in 0..8 {
			for y in 0..8 {
				if let Some(piece) = &mut self.board[y as usize][x as usize].piece {
					piece.post_turn();
				}
			}
		}
	}
}

impl Board {
	pub fn new(white_player: u64, black_player: u64) -> Self {
		Self {
			turn: white_player,
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
