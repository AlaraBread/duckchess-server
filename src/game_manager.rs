use crate::board::Game;
use rocket::{fairing::AdHoc, futures::lock::Mutex};
use std::{
	collections::{HashMap, VecDeque},
	sync::atomic::{AtomicU64, Ordering},
};

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game manager", |rocket| async {
		rocket.manage(GameManager::default())
	})
}

#[derive(Default)]
pub struct GameManager {
	waiting_games: Mutex<VecDeque<u64>>,
	games: Mutex<HashMap<u64, Game>>,
	game_counter: AtomicU64,
}

impl GameManager {
	pub async fn find_match(&self, player_id: u64) -> u64 {
		let mut waiting_games = self.waiting_games.lock().await;
		let mut games = self.games.lock().await;
		// maybe make some better matchmaking at some point
		match waiting_games.iter().find(|game_id| {
			games
				.get(&game_id)
				.map(|game| !game.has_player(player_id))
				.unwrap_or(false)
		}) {
			Some(game_id) => {
				games.get_mut(&game_id).unwrap().join(player_id);
				return *game_id;
			}
			None => {
				let id = self.game_counter.fetch_add(1, Ordering::Relaxed) + 1;
				let mut game = Game::default();
				game.join(player_id);
				games.insert(id, game);
				waiting_games.push_back(id);
				return id;
			}
		}
	}
	pub async fn get_game(&self, game_id: u64) -> Option<Game> {
		return self
			.games
			.lock()
			.await
			.get(&game_id)
			.map(|game| game.clone());
	}
}
