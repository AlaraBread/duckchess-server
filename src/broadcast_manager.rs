use std::{collections::HashMap, sync::Arc};

use rocket::{
	fairing::AdHoc,
	futures::lock::Mutex,
	tokio::sync::broadcast::{channel, Receiver, Sender},
};

use crate::play::PlayResponse;

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game", |rocket| async {
		rocket.manage(Arc::new(BroadcastManager::new()))
	})
}

pub struct BroadcastManager {
	pub channels: Mutex<HashMap<u64, Arc<Sender<PlayResponse>>>>,
}

impl BroadcastManager {
	fn new() -> Self {
		BroadcastManager {
			channels: Mutex::new(HashMap::new()),
		}
	}
	pub async fn new_channel(&self, game_id: u64) {
		let mut channels = self.channels.lock().await;
		let channel = channel::<PlayResponse>(32);
		channels.insert(game_id, Arc::new(channel.0));
	}
	pub async fn remove(&self, game_id: u64) {
		let mut channels = self.channels.lock().await;
		channels.remove(&game_id);
	}
	pub async fn get_sender(&self, game_id: u64) -> Option<Arc<Sender<PlayResponse>>> {
		let channels = self.channels.lock().await;
		return channels.get(&game_id).map(|channel| channel.clone());
	}
	pub async fn listen_to(&self, game_id: u64) -> Option<Receiver<PlayResponse>> {
		let channels = self.channels.lock().await;
		return channels.get(&game_id).map(|recv| recv.subscribe());
	}
}
