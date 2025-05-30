use rocket::serde::{Deserialize, Serialize};

use crate::{Board, Move, Vec2};

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
	SelfInfo {
		// the reciever's player id
		id: String,
	},
	GameState {
		board: Board,
	},
	TurnStart {
		turn: String,
		move_pieces: Vec<Vec2>,
		moves: Vec<Vec<Move>>,
	},
	Move {
		moves: Vec<Move>,
	},
	End {
		winner: Option<String>,
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
	pub turn: String,
	pub move_pieces: Vec<Vec2>,
	pub moves: Vec<Vec<Move>>,
}
