use std::collections::HashMap;

use crate::{
	board::Board,
	broadcast_manager::BroadcastManager,
	play::{GameState, PlayResponse},
};

#[derive(Debug)]
pub struct Game {
	pub listening_players: HashMap<u64, i32>,
	pub players: Vec<u64>,
	pub board: Option<Board>,
	pub cleanup_counters: HashMap<u64, u64>,
	pub started: bool,
	pub id: u64,
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
	pub async fn start(&mut self, broadcast_manager: &BroadcastManager) {
		self.started = true;
		self.board = Some(Board::new(self.players[0], self.players[1]));
		if let Some(broadcast) = broadcast_manager.get_sender(self.id).await {
			let _ = broadcast.send(PlayResponse::Start {
				state: self.get_game_state(),
			});
			let board = self.board.as_mut().expect("just set the board");
			board.generate_moves();
			let _ = broadcast.send(board.turn_message());
		}
	}
	pub fn get_game_state(&self) -> GameState {
		GameState {
			board: self.board.clone(),
			players: self.players.clone(),
			started: self.started,
			listening_players: self
				.listening_players
				.iter()
				.filter(|(_player_id, listen_count)| **listen_count > 0)
				.map(|(player_id, _listen_count)| *player_id)
				.collect(),
		}
	}
}

impl Game {
	pub fn new(id: u64) -> Self {
		Self {
			players: Default::default(),
			listening_players: Default::default(),
			cleanup_counters: Default::default(),
			board: Default::default(),
			started: false,
			id,
		}
	}
}
