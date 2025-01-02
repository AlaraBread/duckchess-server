use std::sync::Arc;

use rocket::{
	fairing::AdHoc,
	futures::{SinkExt, StreamExt},
	get,
	http::CookieJar,
	post, routes,
	serde::json::{json, serde_json, Json},
	tokio::{
		self,
		sync::broadcast::{error::RecvError, Sender},
	},
	Responder, Shutdown, State,
};
use serde::{Deserialize, Serialize};
use ws::{stream::DuplexStream, Channel, WebSocket};

use crate::{
	broadcast_manager::BroadcastManager, game::Tile, game_manager::GameManager,
	player_manager::PlayerManager,
};

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game", |rocket| async {
		rocket.mount("/play", routes![play, find_match])
	})
}

#[post("/find_match")]
async fn find_match(
	cookies: &CookieJar<'_>,
	game_manager: &State<Arc<GameManager>>,
	player_manager: &State<PlayerManager>,
	broadcast_manager: &State<Arc<BroadcastManager>>,
) -> Json<u64> {
	let player_id = player_manager.get_player_id(cookies);
	let game_id = game_manager
		.find_match(player_id, (*broadcast_manager).clone())
		.await;
	let broadcast = broadcast_manager
		.get_sender(game_id)
		.await
		.expect("just created the game");
	let _ = broadcast.send(PlayResponse::PlayerJoined { id: player_id });
	return Json(game_id);
}

#[get("/<game_id>")]
async fn play(
	game_id: u64,
	ws: WebSocket,
	player_manager: &State<PlayerManager>,
	game_manager: &State<Arc<GameManager>>,
	broadcast_manager: &State<Arc<BroadcastManager>>,
	cookies: &CookieJar<'_>,
	mut end: Shutdown,
) -> Result<Channel<'static>, ErrorResponse> {
	let broadcast_manager = (*broadcast_manager).clone();
	let mut receiver = match broadcast_manager.listen_to(game_id).await {
		Some(r) => r,
		None => return Err(ErrorResponse::NotFound(())),
	};
	let player_id = player_manager.get_player_id(cookies);
	let game_manager = (*game_manager).clone();
	Ok(ws.channel(move |mut socket| {
		Box::pin(async move {
			game_manager.update_listeners(game_id, player_id, 1).await;
			send_game_state(&mut socket, &game_manager, game_id).await;
			let close_message;
			loop {
				tokio::select! {
					msg = receiver.recv() => {
						match msg {
							Ok(msg) => {
								let _ = socket.send(ws::Message::Text(json!(msg).to_string())).await;
							},
							Err(RecvError::Closed) => {
								close_message = "game closed";
								break;
							}
							Err(RecvError::Lagged(_)) => continue,
						};
					}
					Some(Ok(message)) = socket.next() => {
						match message {
							ws::Message::Text(text) => {
								let broadcast = match broadcast_manager.get_sender(game_id).await {
									Some(s) => s,
									None => continue,
								};
								handle_play_request(&text, &mut socket, &broadcast).await;
							}
							ws::Message::Close(_) => {
								close_message = "client disconnected";
								break;
							}
							_ => {}
						}
					}
					_ = &mut end => {
						close_message = "server closed";
						break;
					}
					else => {
						close_message = "client disconnected";
						break;
					}
				}
			}
			game_manager.update_listeners(game_id, player_id, -1).await;
			game_manager.cleanup(game_id, player_id, broadcast_manager);
			let close_frame = ws::frame::CloseFrame {
				code: ws::frame::CloseCode::Normal,
				reason: close_message.to_string().into(),
			};
			let _ = socket.close(Some(close_frame)).await;
			Ok(())
		})
	}))
}

#[derive(Deserialize, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PlayRequest {
	Turn,
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PlayResponse {
	PlayerJoined { id: u64 },
	Turn,
	InvalidJSON,
}

async fn handle_play_request(
	text: &str,
	socket: &mut DuplexStream,
	broadcast: &Sender<PlayResponse>,
) {
	let request: PlayRequest = match serde_json::from_str(&text) {
		Ok(m) => m,
		Err(_) => {
			// client sent invalid request
			let _ = socket
				.send(ws::Message::text(
					serde_json::to_string(&PlayResponse::InvalidJSON).unwrap(),
				))
				.await;
			return;
		}
	};
	match request {
		PlayRequest::Turn => {
			let _ = broadcast.send(PlayResponse::Turn);
		}
	};
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
struct GameState {
	pub players: Vec<u64>,
	pub listening_players: Vec<u64>,
	pub board: [[Tile; 8]; 8],
}

async fn send_game_state(socket: &mut DuplexStream, game_manager: &GameManager, game_id: u64) {
	let games = game_manager.games.lock().await;
	let game = match games.get(&game_id) {
		Some(game) => game,
		None => return,
	};
	let _ = socket
		.send(ws::Message::text(
			serde_json::to_string(&GameState {
				board: game.board.clone(),
				players: game.players.clone(),
				listening_players: game
					.listening_players
					.iter()
					.filter(|(_player_id, listen_count)| **listen_count > 0)
					.map(|(player_id, _listen_count)| *player_id)
					.collect(),
			})
			.unwrap(),
		))
		.await;
}

#[derive(Responder)]
enum ErrorResponse {
	#[response(status = 403)]
	Forbidden(()),
	#[response(status = 404)]
	NotFound(()),
}
