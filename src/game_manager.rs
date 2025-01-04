use crate::{broadcast_manager::BroadcastManager, game::Game};
use rocket::{
	fairing::AdHoc,
	futures::lock::Mutex,
	tokio::{task, time},
};
use std::{
	collections::{HashMap, VecDeque},
	future::Future,
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
	pub async fn find_match(
		&self,
		player_id: u64,
		broadcast_manager: Arc<BroadcastManager>,
	) -> u64 {
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
				games
					.get_mut(&game_id)
					.expect("just found game in waiting_games")
					.join(player_id);
				return game_id;
			}
			None => {
				let id = self.game_counter.fetch_add(1, Ordering::Relaxed) + 1;
				let mut game = Game::new(id);
				game.join(player_id);
				games.insert(id, game);
				waiting_games.push_back(id);
				broadcast_manager.new_channel(id).await;
				return id;
			}
		}
	}
	pub async fn add_game_to_queue(&self, game_id: u64) {
		let mut waiting = self.waiting_games.lock().await;
		waiting.push_front(game_id);
	}
	pub async fn update_listeners(&self, game_id: u64, player_id: u64, change: i32) -> i32 {
		let mut games = self.games.lock().await;
		let game = games.get_mut(&game_id);
		if let Some(game) = game {
			return game.update_listeners(player_id, change);
		}
		return 0;
	}
	pub fn cleanup<F, Fut>(self: Arc<Self>, game_id: u64, player_id: u64, cleanup_fn: F)
	where
		F: FnOnce() -> Fut + Send + 'static,
		Fut: Future<Output = ()> + Send,
	{
		task::spawn(async move {
			let mut games = self.games.lock().await;
			let game = match games.get_mut(&game_id) {
				Some(game) => game,
				None => {
					return;
				}
			};
			let counter = game.cleanup_counters.entry(player_id).or_insert(0);
			*counter += 1;
			let counter = *counter;
			drop(games);
			time::sleep(Duration::from_secs(10)).await;
			let mut games = self.games.lock().await;
			let game = match games.get_mut(&game_id) {
				Some(game) => game,
				None => {
					return;
				}
			};
			let new_counter = *game
				.cleanup_counters
				.get(&player_id)
				.expect("just inserted");
			drop(games);
			if new_counter != counter {
				return;
			}
			cleanup_fn().await;
		});
	}
}
