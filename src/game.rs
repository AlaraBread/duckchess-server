use std::{collections::HashMap, sync::Arc};

use rocket::{
	fairing::AdHoc,
	futures::lock::Mutex,
	get,
	http::CookieJar,
	post,
	response::{
		stream::{Event, EventStream},
		Responder,
	},
	routes,
	serde::{json::Json, Serialize},
	tokio::{
		select,
		sync::broadcast::{channel, error::RecvError, Sender},
	},
	Shutdown, State,
};

use crate::{board::Tile, game_manager::GameManager, player_manager::PlayerManager};

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game", |rocket| async {
		rocket
			.manage(Arc::new(channel::<ListenMessage>(1024).0))
			.mount("/game", routes![listen, turn, find_match, get_game_state])
	})
}

#[post("/find_match")]
async fn find_match(
	cookies: &CookieJar<'_>,
	game_manager: &State<Arc<GameManager>>,
	player_manager: &State<PlayerManager>,
	listen_manager: &State<Arc<ListenManager>>,
) -> Json<u64> {
	let player_id = player_manager.get_player_id(cookies);
	let game_id = game_manager.find_match(player_id).await;
	let queue = listen_manager
		.get_channel(game_id)
		.await
		.expect("just created the game");
	let _ = queue.send(ListenMessage {
		player_id: None,
		response: ListenResponse::PlayerJoined { id: player_id },
	});
	return Json(game_id);
}

#[post("/<game_id>/turn")]
async fn turn(
	game_id: u64,
	listen_manager: &State<Arc<ListenManager>>,
	game_manager: &State<Arc<GameManager>>,
	player_manager: &State<PlayerManager>,
	cookies: &CookieJar<'_>,
) -> Result<(), ErrorResponse> {
	let games = game_manager.games.lock().await;
	let game = games.get(&game_id);
	match game {
		Some(game) => {
			if !game.has_player(player_manager.get_player_id(cookies)) {
				return Err(ErrorResponse::Forbidden(()));
			}
			return match listen_manager.get_channel(game_id).await {
				Some(channel) => {
					let _ = channel.send(ListenMessage {
						player_id: None,
						response: ListenResponse::Turn {},
					});
					Ok(())
				}
				None => Err(ErrorResponse::NotFound(())),
			};
		}
		None => {
			return Err(ErrorResponse::NotFound(()));
		}
	};
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
struct GameState {
	pub players: Vec<u64>,
	pub board: [[Tile; 8]; 8],
}

#[get("/<game_id>")]
async fn get_game_state(
	game_id: u64,
	game_manager: &State<Arc<GameManager>>,
	cookies: &CookieJar<'_>,
	player_manager: &State<PlayerManager>,
) -> Result<Json<GameState>, ErrorResponse> {
	let games = game_manager.games.lock().await;
	let game = games.get(&game_id);
	match game {
		Some(game) => {
			if !game.has_player(player_manager.get_player_id(cookies)) {
				return Err(ErrorResponse::Forbidden(()));
			}
			return Ok(Json(GameState {
				board: game.board.clone(),
				players: game.players.clone(),
			}));
		}
		None => {
			return Err(ErrorResponse::NotFound(()));
		}
	};
}

#[derive(Clone, Debug)]
pub struct ListenMessage {
	pub player_id: Option<u64>,
	pub response: ListenResponse,
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum ListenResponse {
	PlayerJoined { id: u64 },
	Closed,
	Turn,
}

#[get("/<game_id>/listen")]
async fn listen(
	game_id: u64,
	listen_manager: &State<Arc<ListenManager>>,
	mut end: Shutdown,
	cookies: &CookieJar<'_>,
	game_manager: &State<Arc<GameManager>>,
	player_manager: &State<PlayerManager>,
) -> Result<EventStream![], ErrorResponse> {
	let game_manager = (*game_manager).clone();
	let listen_manager = (*listen_manager).clone();
	let player_id = player_manager.get_player_id(cookies);
	let mut games = game_manager.games.lock().await;
	match games.get_mut(&game_id) {
		Some(game) => {
			if !game.has_player(player_id) {
				return Err(ErrorResponse::Forbidden(()));
			}
			game.update_listeners(player_id, 1);
		}
		None => return Err(ErrorResponse::NotFound(())),
	}
	drop(games);
	let channels = listen_manager.channels.lock().await;
	let queue = channels.get(&game_id).unwrap().clone();
	drop(channels);
	let mut rx = queue.subscribe();
	Ok(EventStream! {
		loop {
			let msg = select! {
				msg = rx.recv() => match msg {
					Ok(msg) => msg,
					Err(RecvError::Closed) => break,
					Err(RecvError::Lagged(_)) => continue,
				},
				_ = &mut end => break,
			};
			if let Some(recv_id) = msg.player_id {
				if recv_id != player_id {
					continue;
				}
			}
			yield Event::json(&msg.response);
		}
		// cleanup
		let mut games = game_manager.games.lock().await;
		match games.get_mut(&game_id) {
			Some(game) => {
				game.update_listeners(player_id, -1);
			}
			None => return,
		}
		let channels = listen_manager.channels.lock().await;
		let queue = channels.get(&game_id).unwrap();
		game_manager.clone().cleanup(game_id, player_id, queue.clone());
	})
}

#[derive(Responder)]
enum ErrorResponse {
	#[response(status = 403)]
	Forbidden(()),
	#[response(status = 404)]
	NotFound(()),
}

struct ListenManager {
	channels: Mutex<HashMap<u64, Arc<Sender<ListenMessage>>>>,
}

impl ListenManager {
	async fn new_channel(&self, game_id: u64) {
		let mut channels = self.channels.lock().await;
		channels.insert(game_id, Arc::new(channel::<ListenMessage>(32).0));
	}
	async fn get_channel(&self, game_id: u64) -> Option<Arc<Sender<ListenMessage>>> {
		let channels = self.channels.lock().await;
		return channels.get(&game_id).map(|channel| channel.clone());
	}
}
