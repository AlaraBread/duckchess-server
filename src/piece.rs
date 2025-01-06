use rocket::serde::Serialize;

use crate::{
	board::{Board, Move, MoveType, Player},
	vec2::Vec2,
};

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PieceType {
	King,
	Queen,
	Castle,
	Bishop,
	Knight,
	Pawn {
		// used for en passant
		turns_since_double_advance: Option<i32>,
	},
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct Piece {
	pub piece_type: PieceType,
	pub owner: Player,
	pub has_moved: bool,
}

impl Piece {
	fn generate_simple_moves(
		&self,
		offsets: &[Vec2],
		limit: i8,
		pos: Vec2,
		move_type: MoveType,
		board: &Board,
	) -> Vec<Move> {
		let mut moves = Vec::new();
		for dir in offsets {
			let mut pos = pos;
			pos += dir;
			let mut limit = limit;
			while pos.is_inside_board() && limit > 0 {
				if let Some(blocking) = &board.get_tile(pos).piece {
					if blocking.owner != self.owner {
						// capture
						moves.push(Move {
							move_type,
							from: pos,
							to: pos,
						});
					}
					break;
				}
				moves.push(Move {
					move_type,
					from: pos,
					to: pos,
				});
				pos += dir;
				limit -= 1;
			}
		}
		return moves;
	}
	pub fn generate_moves(&self, board: &Board, pos: Vec2) -> Vec<Move> {
		if self.owner != board.turn {
			return vec![];
		}
		return match self.piece_type {
			PieceType::King => self.generate_simple_moves(
				&[
					Vec2(0, 1),
					Vec2(0, -1),
					Vec2(1, 0),
					Vec2(-1, 0),
					Vec2(-1, -1),
					Vec2(-1, 1),
					Vec2(1, -1),
					Vec2(1, 1),
				],
				1,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Queen => self.generate_simple_moves(
				&[
					Vec2(0, 1),
					Vec2(0, -1),
					Vec2(1, 0),
					Vec2(-1, 0),
					Vec2(-1, -1),
					Vec2(-1, 1),
					Vec2(1, -1),
					Vec2(1, 1),
				],
				i8::MAX,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Castle => self.generate_simple_moves(
				&[Vec2(0, 1), Vec2(0, -1), Vec2(1, 0), Vec2(-1, 0)],
				i8::MAX,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Bishop => self.generate_simple_moves(
				&[Vec2(-1, -1), Vec2(-1, 1), Vec2(1, -1), Vec2(1, 1)],
				i8::MAX,
				pos,
				MoveType::SlidingMove,
				board,
			),
			PieceType::Knight => self.generate_simple_moves(
				&[
					Vec2(2, 1),
					Vec2(2, -1),
					Vec2(-2, 1),
					Vec2(-2, -1),
					Vec2(1, 2),
					Vec2(1, -2),
					Vec2(-1, 2),
					Vec2(-1, -2),
				],
				1,
				pos,
				MoveType::JumpingMove,
				board,
			),
			PieceType::Pawn { .. } => {
				let limit = if self.has_moved { 1 } else { 2 };
				let dir = match self.owner {
					Player::White => Vec2(0, -1),
					Player::Black => Vec2(0, 1),
				};
				// advance by 1 and 2
				let mut moves = (1..=limit)
					.map(|i| Move {
						move_type: MoveType::SlidingMove,
						from: pos,
						to: pos + dir * i,
					})
					.filter(|m| {
						if !m.to.is_inside_board() {
							return false;
						}
						let piece = &board.get_tile(m.to).piece;
						if let Some(_) = piece {
							false
						} else {
							true
						}
					})
					.collect::<Vec<Move>>();
				// capture moves
				for side in [Vec2(-1, 0), Vec2(1, 0)] {
					let to = pos + dir + side;
					if let Some(piece) = &board.get_tile(to).piece {
						if piece.owner != self.owner {
							moves.push(Move {
								move_type: MoveType::SlidingMove,
								from: pos,
								to,
							});
						}
					}
				}
				// en passant captures
				for side in [Vec2(-1, 0), Vec2(1, 0)] {
					match &board.get_tile(pos + side).piece {
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
									from: pos,
									to: pos + side + dir,
								})
							}
						}
						_ => {}
					}
				}
				return moves;
			}
		}
		.into_iter()
		.filter(|m| !m.would_cause_lose(board))
		.collect();
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
