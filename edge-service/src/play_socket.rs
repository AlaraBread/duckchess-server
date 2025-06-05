use std::time::Duration;

use duckchess_common::{
	Board, BoardSetup, ChatMessage, GameStart, GameStartPlayer, Move, PlayRequest, PlayResponse,
	Player, Turn, TurnStart,
};
use redis::AsyncCommands;
use redis::streams::StreamId;

use rocket::serde::json::serde_json;
use rocket::time::OffsetDateTime;
use rocket::tokio;
use rocket::{
	futures::SinkExt,
	serde::{Deserialize, Serialize},
};
use rocket_db_pools::sqlx::Row;
use rocket_db_pools::{Connection, sqlx};
use uuid::{NoContext, Timestamp, Uuid};
use ws::stream::DuplexStream;

use crate::util::{close_socket, randomly_permute_2};
use crate::{PostgresPool, RedisPool};
pub struct PlaySocket {
	pub user_id: String,
	pub state: PlaySocketState,
	pub socket: DuplexStream,
	pub db: Connection<PostgresPool>,
	pub redis: Connection<RedisPool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
pub enum PlaySocketState {
	WaitingForSetup {
		last_message: Option<String>,
	},
	Matchmaking {
		elo: f32,
		elo_range: f32,
		setup: BoardSetup,
		last_message: Option<String>,
	},
	Game {
		game_id: String,
		my_turn: bool,
		player: Player,
		last_message: Option<String>,
	},
}

impl PlaySocket {
	pub async fn new(
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
		let mut state = match cached_state {
			Some(state) => Self {
				user_id,
				state,
				socket,
				db,
				redis,
			},
			None => {
				// no cached state, create a new one
				let count: i64 = sqlx::query("SELECT count(id) FROM users WHERE id = $1")
					.bind(&user_id)
					.fetch_one(&mut **db)
					.await
					.expect("postgres error")
					.get(0);
				if count <= 0 {
					return Err(("user not found".to_string(), socket));
				}
				let mut state = Self {
					user_id,
					state: PlaySocketState::WaitingForSetup { last_message: None },
					socket,
					db,
					redis,
				};
				state.save_state().await;
				state
			}
		};
		Self::set_disconnect_snowflake(&state.user_id, &mut state.redis).await;
		state.send_game_state().await;
		state.matchmake().await;
		Ok(state)
	}
	pub async fn disconnected(
		mut self: Self,
		close_message: &str,
		allow_reconnect: bool,
		surrender: bool,
	) {
		// leave matchmaking queue immidiately to prevent getting matched while disconnected
		Self::leave_matchmaking_queue(&self.user_id, &mut self.db).await;
		close_socket(self.socket, close_message.to_string()).await;
		let user_id = self.user_id;
		let mut redis = self.redis;
		let mut db = self.db;
		let state = self.state;
		if allow_reconnect {
			tokio::spawn(async move {
				let disconnect_snowflake =
					Self::set_disconnect_snowflake(&user_id, &mut redis).await;
				tokio::time::sleep(Duration::from_secs(5)).await;
				// if the snowflake changed during the sleep,
				// another socket for the same player joined or left while we were sleeping.
				// we will let that socket handle the cleanup
				if let Some(new_snowflake) =
					Self::get_disconnect_snowflake(&user_id, &mut redis).await
				{
					if new_snowflake == disconnect_snowflake {
						// cleanup
						Self::cleanup(state, &user_id, &mut redis, &mut db, surrender).await;
					}
				}
			});
		} else {
			Self::cleanup(state, &user_id, &mut redis, &mut db, surrender).await;
		}
	}
	async fn cleanup(
		state: PlaySocketState,
		user_id: &str,
		redis: &mut Connection<RedisPool>,
		db: &mut Connection<PostgresPool>,
		forfeit: bool,
	) {
		Self::leave_matchmaking_queue(user_id, db).await;
		let _: usize = redis
			.del(&[
				format!("socket_state:{}", user_id),
				format!("user:{}", user_id),
				format!("disconnect_snowflake:{}", user_id),
			])
			.await
			.expect("redis error");
		if let PlaySocketState::Game { game_id, .. } = state {
			if forfeit {
				// forfeit the game (game service handles game cleanup)
				let _: () = redis
					.xadd_maxlen(
						"game_requests",
						redis::streams::StreamMaxlen::Approx(10000),
						"*",
						&[(
							"forfeit",
							&serde_json::to_string(&(&game_id, user_id))
								.expect("failed to serialize forfeit"),
						)],
					)
					.await
					.expect("redis error");
			}
		}
	}
	async fn set_disconnect_snowflake(user_id: &str, redis: &mut Connection<RedisPool>) -> String {
		let disconnect_snowflake = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
		let _: () = redis
			.set(
				format!("disconnect_snowflake:{}", user_id),
				&disconnect_snowflake,
			)
			.await
			.expect("redis error");
		return disconnect_snowflake;
	}
	async fn get_disconnect_snowflake(
		user_id: &str,
		redis: &mut Connection<RedisPool>,
	) -> Option<String> {
		redis
			.get::<String, String>(format!("disconnect_snowflake:{}", user_id))
			.await
			.ok()
	}
	pub async fn save_state(&mut self) {
		let state = serde_json::to_string(&self.state).expect("couldnt serialize state");
		let _: () = self
			.redis
			.set(format!("socket_state:{}", &self.user_id), state)
			.await
			.expect("redis error");
	}
	pub async fn matchmake(&mut self) {
		if let PlaySocketState::Matchmaking {
			elo,
			elo_range,
			setup,
			..
		} = &mut self.state
		{
			// find longest waiting player where they're in my elo range and im in theirs
			// time complexity isnt a huge deal here because matchmaking_players will remain relatively small
			let match_found = match sqlx::query(
				"SELECT id, board_setup FROM matchmaking_players WHERE \
				elo BETWEEN $1 AND $2 AND \
				$3 BETWEEN elo - elo_range AND elo + elo_range AND \
				id != $4 \
				ORDER BY start_time ASC LIMIT 1",
			)
			.bind(*elo - *elo_range)
			.bind(*elo + *elo_range)
			.bind(*elo)
			.bind(&self.user_id)
			.fetch_one(&mut **self.db)
			.await
			{
				Ok(row) => row,
				Err(_) => {
					// no match
					Self::enter_matchmaking_queue(
						&self.user_id,
						&setup,
						&mut self.db,
						*elo,
						*elo_range,
					)
					.await;
					return;
				}
			};
			let matched_player: String = match_found.get(0);
			let matched_board_setup: BoardSetup = serde_json::from_str(match_found.get(1))
				.expect("invalid board setup in matchmaking queue");
			sqlx::query("DELETE FROM matchmaking_players WHERE id = $1 OR id = $2")
				.bind(&matched_player)
				.bind(&self.user_id)
				.execute(&mut **self.db)
				.await
				.expect("postgres error");
			let game_id = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
			// let the other player know they just got matched
			let (white, black) = randomly_permute_2((
				GameStartPlayer {
					id: matched_player,
					setup: matched_board_setup,
				},
				GameStartPlayer {
					id: self.user_id.clone(),
					setup: setup.clone(),
				},
			));
			let _: () = self
				.redis
				.xadd_maxlen(
					"game_requests",
					redis::streams::StreamMaxlen::Approx(10000),
					"*",
					&[(
						"game_start",
						&serde_json::to_string(&GameStart {
							game_id: game_id.clone(),
							white,
							black,
						})
						.expect("failed to serialize game start"),
					)],
				)
				.await
				.expect("redis error");
		}
	}
	async fn enter_matchmaking_queue(
		user_id: &str,
		board_setup: &BoardSetup,
		db: &mut Connection<PostgresPool>,
		elo: f32,
		elo_range: f32,
	) {
		Self::leave_matchmaking_queue(user_id, db).await;
		sqlx::query(
			"INSERT INTO matchmaking_players \
						(id, elo, elo_range, start_time, board_setup) \
						VALUES ($1, $2, $3, $4, $5)",
		)
		.bind(&user_id)
		.bind(elo)
		.bind(elo_range)
		.bind(OffsetDateTime::now_utc())
		.bind(serde_json::to_string(board_setup).expect("failed to serialize board setup"))
		.execute(&mut ***db)
		.await
		.expect("postgres error");
	}
	async fn leave_matchmaking_queue(user_id: &str, db: &mut Connection<PostgresPool>) {
		sqlx::query("DELETE FROM matchmaking_players WHERE id = $1")
			.bind(&user_id)
			.execute(&mut ***db)
			.await
			.expect("postgres error");
	}
	pub async fn expand_elo_range(&mut self) {
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
	pub async fn game_start(&mut self, game_start: String) {
		let game_start: GameStart =
			serde_json::from_str(&game_start).expect("failed to parse game start");
		self.state = PlaySocketState::Game {
			game_id: game_start.game_id,
			my_turn: false,
			player: match self.user_id == game_start.white.id {
				true => Player::White,
				false => Player::Black,
			},
			last_message: match &self.state {
				PlaySocketState::Game { last_message, .. } => last_message.clone(),
				_ => None,
			},
		};
		self.send_game_state().await;
	}
	pub async fn turn_start(&mut self, turn_start: String) {
		let turn_start: TurnStart =
			serde_json::from_str(&turn_start).expect("failed to parse turn start");
		if let PlaySocketState::Game {
			my_turn, player, ..
		} = &mut self.state
		{
			*my_turn = turn_start.turn == *player;
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
			if *my_turn {
				Self::send_chat_message(
					&mut self.socket,
					ChatMessage {
						id: "".to_string(),
						message: "your turn".to_string(),
					},
				)
				.await;
			}
		}
	}
	pub async fn moves_recieved(&mut self, moves: String) {
		let moves: Vec<Move> = serde_json::from_str(&moves).expect("failed to parse moves");
		let _ = self
			.socket
			.send(ws::Message::Text(
				serde_json::to_string(&PlayResponse::Move { moves })
					.expect("failed to serialize moves"),
			))
			.await;
	}
	pub async fn chat_recieved(&mut self, message: String) {
		let chat_message: ChatMessage =
			serde_json::from_str(&message).expect("failed to parse chat message");
		if chat_message.id != self.user_id {
			Self::send_chat_message(&mut self.socket, chat_message).await;
		}
	}
	async fn send_chat_message(socket: &mut DuplexStream, chat_message: ChatMessage) {
		let _ = socket
			.send(ws::Message::Text(
				serde_json::to_string(&PlayResponse::ChatMessage {
					message: chat_message,
				})
				.expect("failed to serialize chat message"),
			))
			.await;
	}
	pub async fn game_end(&mut self, winner: String) {
		let _ = self
			.socket
			.send(ws::Message::Text(
				serde_json::to_string(&PlayResponse::End { winner })
					.expect("failed to serialize game end"),
			))
			.await;
	}
	// handle message from user
	pub async fn handle_message(&mut self, message: &str) -> bool {
		let message: PlayRequest = match serde_json::from_str(message) {
			Ok(message) => message,
			Err(_) => return false,
		};
		match message {
			PlayRequest::Turn {
				piece_idx,
				move_idx,
			} => {
				if let PlaySocketState::Game {
					game_id, my_turn, ..
				} = &mut self.state
				{
					if !*my_turn {
						return false;
					}
					*my_turn = false;
					let _: () = self
						.redis
						.xadd_maxlen(
							"game_requests",
							redis::streams::StreamMaxlen::Approx(10000),
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
					self.save_state().await;
				}
			}
			PlayRequest::ChatMessage { message } => {
				if message.len() > 1024 {
					return false;
				}
				if let PlaySocketState::Game { game_id, .. } = &self.state {
					let chat_message = ChatMessage {
						id: self.user_id.clone(),
						message,
					};
					let message = serde_json::to_string(&chat_message)
						.expect("failed to serialize chat message");
					let _: () = self
						.redis
						.xadd_maxlen(
							format!("game:{}", &game_id),
							redis::streams::StreamMaxlen::Approx(1000),
							"*",
							&[("chat", message.as_str())],
						)
						.await
						.expect("redis error");
					Self::send_chat_message(&mut self.socket, chat_message).await;
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
			PlayRequest::BoardSetup { setup } => {
				if let PlaySocketState::WaitingForSetup { .. } = self.state {
					if !setup.is_valid() {
						return false;
					}
					let elo: f32 = sqlx::query("SELECT elo FROM users WHERE id = $1")
						.bind(&self.user_id)
						.fetch_one(&mut **self.db)
						.await
						.expect("postgres error")
						.get::<f32, usize>(0);
					self.state = PlaySocketState::Matchmaking {
						elo,
						elo_range: 200.0,
						setup,
						last_message: None,
					};
					self.matchmake().await;
					self.save_state().await;
				}
			}
			PlayRequest::Surrender => {
				if let PlaySocketState::Game { .. } = &self.state {
					return true;
				}
			}
		}
		false
	}
	pub async fn process_stream_id(&mut self, message: StreamId) -> bool {
		match &mut self.state {
			PlaySocketState::Matchmaking { last_message, .. }
			| PlaySocketState::WaitingForSetup { last_message } => {
				*last_message = Some(message.id.clone());
				self.process_stream_message(message).await
			}
			PlaySocketState::Game { last_message, .. } => {
				*last_message = Some(message.id.clone());
				self.process_stream_message(message).await
			}
		}
	}
	async fn process_stream_message(&mut self, message: StreamId) -> bool {
		if let Some(game_start) = message.get::<String>("game_start") {
			self.game_start(game_start).await;
		}
		if let Some(turn_start) = message.get::<String>("turn_start") {
			self.turn_start(turn_start).await;
		}
		if let Some(moves) = message.get::<String>("moves") {
			self.moves_recieved(moves).await;
		}
		if let Some(chat) = message.get::<String>("chat") {
			self.chat_recieved(chat).await;
		}
		if let Some(winner) = message.get::<String>("end") {
			self.game_end(winner).await;
			false
		} else {
			true
		}
	}
	pub async fn send_game_state(&mut self) {
		if let PlaySocketState::Game { game_id, .. } = &mut self.state {
			let board: Board = serde_json::from_str(
				match &self
					.redis
					.get::<String, String>(format!("board:{}", game_id))
					.await
				{
					Ok(v) => v,
					Err(_) => {
						// game doesnt exist
						self.state = PlaySocketState::WaitingForSetup { last_message: None };
						self.save_state().await;
						return;
					}
				},
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
