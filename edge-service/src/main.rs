mod util;

use duckchess_common::{
	Board, ChatMessage, GameStart, Move, PlayRequest, PlayResponse, Turn, TurnStart,
};
use rand::Rng;
use redis::streams::{StreamKey, StreamReadOptions, StreamReadReply};
use redis::{AsyncCommands, RedisFuture};
use rocket::futures::StreamExt;
use rocket::{Shutdown, get, launch, routes};

use crate::util::{close_socket, conditional_future};
use rocket::http::{Cookie, CookieJar};
use rocket::serde::json::serde_json;
use rocket::time::OffsetDateTime;
use rocket::{
	futures::SinkExt,
	serde::{Deserialize, Serialize},
	tokio,
};
use rocket_db_pools::sqlx::Row;
use rocket_db_pools::{
	Connection, Database,
	deadpool_redis::{self},
	sqlx,
};
use uuid::{NoContext, Timestamp, Uuid};
use ws::stream::DuplexStream;
use ws::{Channel, WebSocket};

#[get("/")]
async fn play(
	ws: WebSocket,
	db: Connection<PostgresPool>,
	redis: Connection<RedisPool>,
	cookies: &CookieJar<'_>,
	mut end: Shutdown,
) -> Channel<'static> {
	let user_id = cookies
		.get_private("user_id")
		.map(|c| c.value().to_string())
		.unwrap_or_else(|| {
			let id = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
			cookies.add_private(Cookie::new("user_id", id.clone()));
			id
		});
	ws.channel(move |socket| {
		Box::pin(async move {
			let mut socket_state = match PlaySocket::new(socket, user_id, db, redis).await {
				Ok(s) => s,
				Err((msg, mut socket)) => {
					close_socket(&mut socket, msg).await;
					return Ok(());
				}
			};
			socket_state.matchmake().await;
			socket_state.send_game_state().await;
			let stream_options = StreamReadOptions::default().block(10000).count(1);
			let matchmaking_stream_key = &[format!("matchmaking:{}", socket_state.user_id)];
			let mut matchmaking_redis = socket_state.redis.clone();
			let mut game_redis = socket_state.redis.clone();
			let close_message;
			loop {
				let matchmaking_stream: RedisFuture<StreamReadReply> = matchmaking_redis
					.xread_options(matchmaking_stream_key, &[">"], &stream_options);
				let game_stream_key;
				let game_stream: Option<RedisFuture<StreamReadReply>> = match &socket_state.state {
					PlaySocketState::Matchmaking { .. } => None,
					PlaySocketState::Game { game_id, .. } => {
						game_stream_key = [format!("game:{}", &game_id)];
						Some(game_redis.xread_options(&game_stream_key, &[">"], &stream_options))
					}
				};
				tokio::select! {
					Some(Ok(message)) = socket_state.socket.next() => {
						match message {
							ws::Message::Text(text) => {
								socket_state.handle_message(&text).await;
							}
							ws::Message::Close(_) => {
								close_message = "client disconnected";
								break;
							}
							_ => {}
						}
					}
					Ok(message) = matchmaking_stream => {
						for StreamKey {ids, ..} in message.keys {
							for message in ids {
								let game_id: String = match message.get("match") {
									Some(m) => m,
									None => continue
								};
								socket_state.start_game(game_id).await;
							}
						}
					}
					Some(Ok(message)) = conditional_future(game_stream) => {
						for StreamKey {ids, ..} in message.keys {
							for message in ids {
								if let Some(turn_start) = message.get::<String>("turn_start") {
									socket_state.turn_start(turn_start).await;
								}
								if let Some(moves) = message.get::<String>("moves") {
									socket_state.moves_recieved(moves).await;
								}
								if let Some(chat) = message.get::<String>("chat") {
									socket_state.chat_recieved(chat).await;
								}
							}
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
			// disconnected
			// if we were the last listener for this player, send player left event
			// wait a second and check if noone is in the game, then clean it up
			close_socket(&mut socket_state.socket, close_message.to_string()).await;
			Ok(())
		})
	})
}

struct PlaySocket {
	user_id: String,
	state: PlaySocketState,
	socket: DuplexStream,
	db: Connection<PostgresPool>,
	redis: Connection<RedisPool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
enum PlaySocketState {
	Matchmaking { elo: f32, elo_range: f32 },
	Game { game_id: String, my_turn: bool },
}

impl PlaySocket {
	async fn new(
		socket: DuplexStream,
		user_id: String,
		mut db: Connection<PostgresPool>,
		mut redis: Connection<RedisPool>,
	) -> Result<Self, (String, DuplexStream)> {
		let cached_state: Option<PlaySocketState> = redis
			.get::<String, String>(format!("socket_state:{}", &user_id))
			.await
			.map(|s| serde_json::from_str(&s).ok())
			.ok()
			.flatten();
		let state = match cached_state {
			Some(state) => Self {
				user_id,
				state,
				socket,
				db,
				redis,
			},
			None => {
				// no cached state, create a new one
				let elo = match sqlx::query("SELECT elo FROM users WHERE id = $1")
					.bind(&user_id)
					.fetch_one(&mut **db)
					.await
				{
					Ok(row) => row,
					Err(_) => return Err(("user not found".to_string(), socket)),
				}
				.get(0);
				let mut state = Self {
					user_id,
					state: PlaySocketState::Matchmaking {
						elo,
						elo_range: 200.0,
					},
					socket,
					db,
					redis,
				};
				state.save_state().await;
				state
			}
		};
		Ok(state)
	}
	async fn save_state(&mut self) {
		let state = serde_json::to_string(&self.state).expect("couldnt serialize state");
		let _: () = self
			.redis
			.set(format!("socket_state:{}", &self.user_id), state)
			.await
			.expect("redis error");
	}
	async fn matchmake(&mut self) {
		if let PlaySocketState::Matchmaking { elo, elo_range } = &mut self.state {
			// find longest waiting player where they're in my elo range and im in theirs
			// time complexity isnt a huge deal here because matchmaking_players will remain relatively small
			let matched_player: String = match sqlx::query(
				"SELECT id FROM matchmaking_players WHERE \
				elo BETWEEN $1 AND $2 AND \
				$3 BETWEEN elo - elo_range AND elo + elo_range \
				ORDER BY start_time ASC LIMIT 1",
			)
			.bind(*elo - *elo_range)
			.bind(*elo + *elo_range)
			.bind(*elo)
			.fetch_one(&mut **self.db)
			.await
			{
				Ok(row) => row,
				Err(_) => {
					// no match
					// add self to matchmaking_players
					sqlx::query("DELETE FROM matchmaking_players WHERE id = $1")
						.bind(&self.user_id)
						.execute(&mut **self.db)
						.await
						.expect("postgres error");
					sqlx::query(
						"INSERT INTO matchmaking_players \
						(id, elo, elo_range, start_time) \
						VALUES ($1, $2, $3, $4)",
					)
					.bind(&self.user_id)
					.bind(*elo)
					.bind(*elo_range)
					.bind(OffsetDateTime::now_utc())
					.execute(&mut **self.db)
					.await
					.expect("postgres error");
					return;
				}
			}
			.get(0);
			sqlx::query("DELETE FROM matchmaking_players WHERE id = $1 OR id = $2")
				.bind(&matched_player)
				.bind(&self.user_id)
				.execute(&mut **self.db)
				.await
				.expect("postgres error");
			let game_id = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
			// let the other player know they just got matched
			let _: () = self
				.redis
				.xadd(
					format!("matchmaking:{}", matched_player),
					"*",
					&[("match", &game_id)],
				)
				.await
				.expect("redis error");
			let (white_player, black_player) = match rand::rng().random() {
				true => (matched_player, self.user_id.clone()),
				false => (self.user_id.clone(), matched_player),
			};
			let _: () = self
				.redis
				.xadd(
					"game_requests",
					"*",
					&[(
						"game_start",
						&serde_json::to_string(&GameStart {
							game_id: game_id.clone(),
							white_player,
							black_player,
						})
						.expect("failed to serialize game start"),
					)],
				)
				.await
				.expect("redis error");
			self.start_game(game_id).await;
		}
	}
	async fn expand_elo_range(&mut self) {
		if let PlaySocketState::Matchmaking { elo_range, .. } = &mut self.state {
			*elo_range *= 2.0;
			sqlx::query("UPDATE matchmaking_players SET elo_range = $1 WHERE id = $2")
				.bind(*elo_range)
				.bind(&self.user_id)
				.execute(&mut **self.db)
				.await
				.expect("postgres error");
			self.matchmake().await;
		}
	}
	async fn start_game(&mut self, game_id: String) {
		self.state = PlaySocketState::Game {
			game_id,
			my_turn: false,
		};
		self.save_state().await;
	}
	async fn turn_start(&mut self, turn_start: String) {
		let turn_start: TurnStart =
			serde_json::from_str(&turn_start).expect("failed to parse turn start");
		if let PlaySocketState::Game { my_turn, .. } = &mut self.state {
			*my_turn = turn_start.turn == self.user_id;
			let _ = self
				.socket
				.send(ws::Message::Text(
					serde_json::to_string(&PlayResponse::TurnStart {
						turn: turn_start.turn,
						move_pieces: turn_start.move_pieces,
						moves: turn_start.moves,
					})
					.expect("failed to serialize turn start"),
				))
				.await;
			self.save_state().await;
		}
	}
	async fn moves_recieved(&mut self, moves: String) {
		let moves: Vec<Move> = serde_json::from_str(&moves).expect("failed to parse moves");
		let _ = self
			.socket
			.send(ws::Message::Text(
				serde_json::to_string(&PlayResponse::Move { moves })
					.expect("failed to serialize moves"),
			))
			.await;
	}
	async fn chat_recieved(&mut self, message: String) {
		let chat_message: ChatMessage =
			serde_json::from_str(&message).expect("failed to parse chat message");
		let _ = self
			.socket
			.send(ws::Message::Text(
				serde_json::to_string(&PlayResponse::ChatMessage {
					message: chat_message,
				})
				.expect("failed to serialize chat message"),
			))
			.await;
	}
	// handle message from user
	async fn handle_message(&mut self, message: &str) {
		let message: PlayRequest = serde_json::from_str(message).expect("failed to parse message");
		match message {
			PlayRequest::Turn {
				piece_idx,
				move_idx,
			} => {
				if let PlaySocketState::Game { game_id, my_turn } = &mut self.state {
					if !*my_turn {
						return;
					}
					let _: () = self
						.redis
						.xadd(
							"game_requests",
							"*",
							&[(
								"turn",
								serde_json::to_string(&Turn {
									game_id: game_id.clone(),
									piece_idx,
									move_idx,
								})
								.expect("failed to serialize turn"),
							)],
						)
						.await
						.expect("redis error");
				}
			}
			PlayRequest::ChatMessage { message } => {
				if message.len() > 1024 {
					return;
				}
				if let PlaySocketState::Game { game_id, .. } = &mut self.state {
					let message = serde_json::to_string(&ChatMessage {
						id: self.user_id.clone(),
						message,
					})
					.expect("failed to serialize chat message");
					let _: () = self
						.redis
						.xadd(
							format!("game:{}", &game_id),
							"*",
							&[("chat", message.as_str())],
						)
						.await
						.expect("redis error");
					let chat_key = format!("chat:{}", game_id);
					let _: usize = self
						.redis
						.rpush(&chat_key, message.as_str())
						.await
						.expect("redis error");
					let _: () = self
						.redis
						.ltrim(&chat_key, -100, -1)
						.await
						.expect("redis error");
				}
			}
			PlayRequest::ExpandEloRange => self.expand_elo_range().await,
		}
	}
	async fn send_game_state(&mut self) {
		if let PlaySocketState::Game { game_id, .. } = &mut self.state {
			let board: Board = serde_json::from_str(
				&self
					.redis
					.get::<String, String>(format!("board:{}", game_id))
					.await
					.expect("redis error"),
			)
			.expect("failed to deserialize board");
			let _ = self
				.socket
				.send(ws::Message::Text(
					serde_json::to_string(&PlayResponse::GameState { board })
						.expect("failed to serialize game state"),
				))
				.await;
			let full_chat: Vec<ChatMessage> = self
				.redis
				.lrange::<String, Vec<String>>(format!("chat:{}", game_id), 0, -1)
				.await
				.expect("redis error")
				.into_iter()
				.map(|m: String| {
					serde_json::from_str(&m).expect("failed to deserialize chat message")
				})
				.collect();
			let _ = self
				.socket
				.send(ws::Message::Text(
					serde_json::to_string(&PlayResponse::FullChat { chat: full_chat })
						.expect("failed to serialize chat messages"),
				))
				.await;
		}
	}
}

#[derive(Database)]
#[database("redis")]
struct RedisPool(deadpool_redis::Pool);

#[derive(Database)]
#[database("postgres")]
struct PostgresPool(sqlx::PgPool);

#[launch]
fn rocket() -> _ {
	rocket::build()
		.mount("/", routes![play])
		.attach(RedisPool::init())
		.attach(PostgresPool::init())
}
