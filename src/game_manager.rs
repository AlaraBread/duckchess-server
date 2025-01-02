use crate::{broadcast_manager::BroadcastManager, game::Game};
use rocket::{
	fairing::AdHoc,
	futures::lock::Mutex,
	tokio::{task, time},
};
use std::{
	collections::{HashMap, VecDeque},
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
	time::Duration,
};

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game manager", |rocket| async {
		rocket.manage(Arc::new(GameManager::default()))
	})
}

#[derive(Default)]
pub struct GameManager {
	waiting_games: Mutex<VecDeque<u64>>,
	pub games: Mutex<HashMap<u64, Game>>,
	game_counter: AtomicU64,
}

impl GameManager {
	pub async fn find_match(&self, player_id: u64, listen_manager: Arc<BroadcastManager>) -> u64 {
		let mut waiting_games = self.waiting_games.lock().await;
		let mut games = self.games.lock().await;
		// maybe make some better matchmaking at some point
		match waiting_games.iter().enumerate().find(|(_idx, game_id)| {
			games
				.get_mut(&game_id)
				.map(|game| game.get_total_listeners() > 0 && game.get_listen_count(player_id) <= 0)
				.unwrap_or(false)
		}) {
			Some((idx, game_id)) => {
				let game_id = *game_id;
				waiting_games.remove(idx);
				games.get_mut(&game_id).unwrap().join(player_id);
				return game_id;
			}
			None => {
				let id = self.game_counter.fetch_add(1, Ordering::Relaxed) + 1;
				let mut game = Game::default();
				game.join(player_id);
				games.insert(id, game);
				waiting_games.push_back(id);
				listen_manager.new_channel(id).await;
				return id;
			}
		}
	}
	pub async fn update_listeners(&self, game_id: u64, player_id: u64, change: i32) -> bool {
		let mut games = self.games.lock().await;
		let game = games.get_mut(&game_id);
		if let Some(game) = game {
			game.update_listeners(player_id, change);
			true
		} else {
			false
		}
	}
	pub fn cleanup(
		self: Arc<Self>,
		game_id: u64,
		player_id: u64,
		listen_manager: Arc<BroadcastManager>,
	) {
		task::spawn(async move {
			let mut games = self.games.lock().await;
			let game = match games.get_mut(&game_id) {
				Some(game) => game,
				None => {
					return;
				}
			};
			let prev_counter = game.cleanup_counter.fetch_add(1, Ordering::Relaxed) + 1;
			drop(games);
			time::sleep(Duration::from_secs(10)).await;
			let mut games = self.games.lock().await;
			let game = match games.get_mut(&game_id) {
				Some(game) => game,
				None => {
					return;
				}
			};
			let new_counter = game.cleanup_counter.load(Ordering::Relaxed);
			if new_counter != prev_counter || game.get_listen_count(player_id) > 0 {
				return;
			}
			games.remove(&game_id);
			listen_manager.remove(game_id).await;
		});
	}
}
