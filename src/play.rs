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
	board::{Board, MoveType},
	broadcast_manager::BroadcastManager,
	game::Game,
	game_manager::GameManager,
	player_manager::PlayerManager,
	vec2::Vec2,
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
	let broadcast_manager = (*broadcast_manager).clone();
	let game_manager = (*game_manager).clone();
	let player_id = player_manager.get_player_id(cookies);
	let game_id = game_manager
		.find_match(player_id, broadcast_manager.clone())
		.await;
	let broadcast = broadcast_manager
		.clone()
		.get_sender(game_id)
		.await
		.expect("just created the game");
	let _ = broadcast.send(PlayResponse::PlayerAdded { id: player_id });
	game_manager
		.clone()
		.cleanup(game_id, player_id, move || async move {
			let mut games = game_manager.games.lock().await;
			let game = match games.get_mut(&game_id) {
				Some(g) => g,
				None => {
					return;
				}
			};
			if game.get_listen_count(player_id) > 0 {
				return;
			}
			if game.get_total_listeners() > 0 {
				if let Some(broadcast) = broadcast_manager.get_sender(game_id).await {
					let _ = broadcast.send(PlayResponse::PlayerRemoved { id: player_id });
				}
				game_manager.add_game_to_queue(game_id).await;
			} else {
				games.remove(&game_id);
				broadcast_manager.remove(game_id).await;
			}
		});
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
	let games = game_manager.games.lock().await;
	match games.get(&game_id) {
		Some(game) => {
			if !game.has_player(player_id) {
				return Err(ErrorResponse::Forbidden(()));
			}
		}
		None => return Err(ErrorResponse::NotFound(())),
	}
	drop(games);
	Ok(ws.channel(move |mut socket| {
		Box::pin(async move {
			let listeners = game_manager.update_listeners(game_id, player_id, 1).await;
			let _ = socket.send(ws::Message::text(
				serde_json::to_string(&PlayResponse::JoinInfo { id: player_id }).unwrap(),
			)).await;
			send_game_state(&mut socket, &game_manager, game_id).await;
			if listeners == 1 {
				// werent listening before and are now
				if let Some(broadcast) = broadcast_manager.get_sender(game_id).await {
					let _ = broadcast.send(PlayResponse::PlayerJoined { id: player_id });
				}
			}
			let mut games = game_manager.games.lock().await;
			let game = games.get_mut(&game_id);
			if let Some(game) = game {
				if !game.started && game.get_total_listeners() >= 2 {
					game.start(&broadcast_manager).await;
				}
			}
			drop(games);
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
								handle_play_request(player_id, game_id, game_manager.clone(), &text, &mut socket, &broadcast).await;
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
			let listeners = game_manager.update_listeners(game_id, player_id, -1).await;
			if listeners == 0 {
				// were listening before and arent anymore
				if let Some(broadcast) = broadcast_manager.get_sender(game_id).await {
					let _ = broadcast.send(PlayResponse::PlayerLeft { id: player_id });
				}
			}
			game_manager
				.clone()
				.cleanup(game_id, player_id, move || async move {
					let mut games = game_manager.games.lock().await;
					let game = match games.get_mut(&game_id) {
						Some(g) => g,
						None => {
							return;
						}
					};
					if game.get_total_listeners() >= 2 {
						return;
					}
					if game.started {
						games.remove(&game_id);
						broadcast_manager.remove(game_id).await;
					} else {
						game_manager.add_game_to_queue(game_id).await;
					}
				});
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
	Turn { piece_idx: usize, move_idx: usize },
	ChatMessage { message: String },
}

#[derive(Serialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PlayResponse {
	InvalidRequest,
	JoinInfo {
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
	Start {
		state: GameState,
	},
	TurnStart {
		turn: u64,
		move_pieces: Vec<Vec2>,
		moves: Vec<Vec<Vec2>>,
	},
	Move {
		move_type: MoveType,
		from: Vec2,
		to: Vec2,
	},
	ChatMessage {
		id: u64,
		message: String,
	},
}

async fn handle_play_request(
	player_id: u64,
	game_id: u64,
	game_manager: Arc<GameManager>,
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
					serde_json::to_string(&PlayResponse::InvalidRequest).unwrap(),
				))
				.await;
			return;
		}
	};
	match request {
		PlayRequest::Turn {
			move_idx,
			piece_idx,
		} => {
			let mut games = game_manager.games.lock().await;
			let board = match games.get_mut(&game_id) {
				Some(Game {
					board: Some(board), ..
				}) => board,
				_ => {
					return;
				}
			};
			if board.get_player_id() != player_id {
				return;
			}
			let move_response = board.execute_move(piece_idx, move_idx);
			if let Some(move_response) = move_response {
				let _ = broadcast.send(move_response);
				board.generate_moves();
				let _ = broadcast.send(board.turn_message());
			} else {
				let _ = socket
					.send(ws::Message::text(
						serde_json::to_string(&PlayResponse::InvalidRequest).unwrap(),
					))
					.await;
			}
		}
		PlayRequest::ChatMessage { message } => {
			let _ = broadcast.send(PlayResponse::ChatMessage {
				id: player_id,
				message,
			});
		}
	};
}

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub struct GameState {
	pub players: Vec<u64>,
	pub listening_players: Vec<u64>,
	pub board: Option<Board>,
	pub started: bool,
}

async fn send_game_state(socket: &mut DuplexStream, game_manager: &GameManager, game_id: u64) {
	let games = game_manager.games.lock().await;
	let game = match games.get(&game_id) {
		Some(game) => game,
		None => return,
	};
	let _ = socket
		.send(ws::Message::text(
			serde_json::to_string(&PlayResponse::GameState {
				state: game.get_game_state(),
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
