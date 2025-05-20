use rocket::serde::{Deserialize, Serialize};

use crate::{Board, Move, Player, Vec2};

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
}

#[derive(Serialize, Clone, Debug)]
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
		id: u64,
	},
	PlayerAdded {
		id: u64,
	},
	PlayerRemoved {
		id: u64,
	},
	PlayerJoined {
		id: u64,
	},
	PlayerLeft {
		id: u64,
	},
	GameState {
		state: GameState,
	},
	TurnStart {
		turn: Player,
		move_pieces: Vec<Vec2>,
		moves: Vec<Vec<Move>>,
	},
	Move {
		moves: Vec<Move>,
	},
	End {
		winner: Option<Player>,
	},
	ChatMessage {
		id: u64,
		message: String,
	},
}

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct GameState {
	pub players: Vec<u64>,
	pub listening_players: Vec<u64>,
	pub board: Option<Board>,
	pub started: bool,
}
