use rocket::serde::{Deserialize, Serialize};

use crate::{
	board::{Board, Move, MoveType, Player},
	vec2::Vec2,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(
	crate = "rocket::serde",
	rename_all = "camelCase",
	rename_all_fields = "camelCase",
	tag = "type"
)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
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
			let mut to = pos;
			to += dir;
			let mut limit = limit;
			while to.is_inside_board() && limit > 0 {
				if let Some(blocking) = &board.get_tile(to).piece {
					if blocking.owner != self.owner {
						// capture
						moves.push(Move {
							move_type: move_type.clone(),
							from: pos,
							to,
						});
					}
					break;
				}
				moves.push(Move {
					move_type: move_type.clone(),
					from: pos,
					to,
				});
				to += dir;
				limit -= 1;
			}
		}
		return moves;
	}
	pub fn generate_moves(&self, board: &Board, pos: Vec2, deep: bool) -> Vec<Move> {
		if self.owner != board.turn {
			return vec![];
		}
		let moves = match self.piece_type {
			PieceType::King => {
				let mut moves = self.generate_simple_moves(
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
				);
				// castling
				if !self.has_moved && (pos.0 == 3 || pos.0 == 4) {
					'castle_position: for castle_position in [Vec2(0, pos.1), Vec2(7, pos.1)] {
						if let Some(Piece {
							piece_type: PieceType::Castle,
							owner,
							has_moved: false,
						}) = &board.get_tile(castle_position).piece
						{
							if *owner != self.owner {
								continue;
							}
							let direction = Vec2(if castle_position.0 == 0 { -1 } else { 1 }, 0);
							let mut cur = pos + direction;
							while (cur + direction).is_inside_board() {
								if board.get_tile(cur).piece.is_some() {
									continue 'castle_position;
								}
								cur += &direction;
							}
							let new_king_position = pos + direction * 2;
							// cant move from, through, or onto an attacked tile
							let mut cur = pos;
							while cur != new_king_position + direction {
								let move_ = Move {
									move_type: MoveType::JumpingMove,
									from: pos,
									to: new_king_position,
								};
								if move_.would_cause_lose(board) {
									continue 'castle_position;
								}
								cur += &direction;
							}
							moves.push(Move {
								move_type: MoveType::Castle {
									from: castle_position,
									to: new_king_position - direction,
								},
								from: pos,
								to: new_king_position,
							});
						}
					}
				}
				moves
			}
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
				let mut moves = Vec::new();
				// advance by 1 and 2
				for i in 1..=limit {
					let to = pos + dir * i;
					if !to.is_inside_board() || board.get_tile(to).piece.is_some() {
						break;
					}
					moves.push(Move {
						move_type: MoveType::SlidingMove,
						from: pos,
						to,
					});
				}
				// capture moves
				for side in [Vec2(-1, 0), Vec2(1, 0)] {
					let to = pos + dir + side;
					if !to.is_inside_board() {
						continue;
					}
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
					let to = pos + side + dir;
					if !to.is_inside_board() {
						continue;
					}
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
									move_type: MoveType::EnPassant,
									from: pos,
									to,
								})
							}
						}
						_ => {}
					}
				}
				moves
					.into_iter()
					.flat_map(|original_move| {
						let final_rank = match self.owner {
							Player::White => 0,
							Player::Black => 7,
						};
						if original_move.to.1 == final_rank {
							// promotion
							let mut moves = Vec::with_capacity(4);
							for into in [
								PieceType::Queen,
								PieceType::Knight,
								PieceType::Bishop,
								PieceType::Castle,
							] {
								moves.push(Move {
									to: original_move.to,
									from: original_move.from,
									move_type: MoveType::Promotion { into },
								});
							}
							moves
						} else {
							vec![original_move]
						}
					})
					.collect()
			}
		};
		return if deep {
			moves
				.into_iter()
				.filter(|m| !m.would_cause_lose(board))
				.collect()
		} else {
			moves
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
