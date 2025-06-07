use crate::{Board, BoardSetup, ChessClock, Move, Player, Vec2};
use rocket::serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
#[serde(
	crate = "rocket::serde",
	rename_all = "camelCase",
	rename_all_fields = "camelCase",
	tag = "type"
)]
pub enum PlayRequest {
	Turn { piece_idx: usize, move_idx: usize },
	ChatMessage { message: String },
	ExpandEloRange,
	BoardSetup { setup: BoardSetup },
	Surrender,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(
	crate = "rocket::serde",
	rename_all = "camelCase",
	rename_all_fields = "camelCase",
	tag = "type"
)]
pub enum PlayResponse {
	InvalidRequest,
	GameState {
		board: Board,
		clock: ChessClock,
	},
	TurnStart {
		turn: Player,
		move_pieces: Vec<Vec2>,
		moves: Vec<Vec<Move>>,
		clock: ChessClock,
	},
	Move {
		moves: Vec<Move>,
	},
	End {
		winner: String,
	},
	ChatMessage {
		message: ChatMessage,
	},
	FullChat {
		chat: Vec<ChatMessage>,
	},
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase")]
pub struct ChatMessage {
	pub id: String,
	pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct TurnStart {
	pub turn: Player,
	pub move_pieces: Vec<Vec2>,
	pub moves: Vec<Vec<Move>>,
}
