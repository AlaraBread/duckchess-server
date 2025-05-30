use std::time::Duration;

use duckchess_common::{
	Board, ChatMessage, GameStart, Move, PlayRequest, PlayResponse, Turn, TurnStart,
};
use rand::Rng;
use redis::AsyncCommands;

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

use crate::util::close_socket;
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
	Matchmaking { elo: f32, elo_range: f32 },
	Game { game_id: String, my_turn: bool },
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
		let _: i64 = state
			.redis
			.incr(format!("user_socket_count:{}", &state.user_id), 1)
			.await
			.expect("redis error");
		Ok(state)
	}
	pub async fn disconnected(mut self: Self, close_message: &str, allow_reconnect: bool) {
		close_socket(&mut self.socket, close_message.to_string()).await;
		let _: i64 = self
			.redis
			.decr(format!("user_socket_count:{}", &self.user_id), 1)
			.await
			.expect("redis error");
		if allow_reconnect {
			tokio::spawn(async move {
				let disconnect_snowflake = self.set_disconnect_snowflake().await;
				tokio::time::sleep(Duration::from_secs(5)).await;
				// if the snowflake changed during the sleep, someone else joined and left while we were sleeping
				// we will let that socket handle the cleanup
				let snowflake_match = self.get_disconnect_snowflake().await == disconnect_snowflake;
				// if another socket still exists for this player, we shouldn't cleanup
				let current_socket_count = self
					.redis
					.get::<String, i64>(format!("user_socket_count:{}", &self.user_id))
					.await
					.expect("redis error");
				if current_socket_count <= 0 && snowflake_match {
					// cleanup
					self.cleanup().await;
				}
			});
		} else {
			self.cleanup().await;
		}
	}
	async fn cleanup(&mut self) {
		sqlx::query("DELETE FROM matchmaking_players WHERE id = $1")
			.bind(&self.user_id)
			.execute(&mut **self.db)
			.await
			.expect("postgres error");
		if let PlaySocketState::Game { game_id, .. } = &self.state {
			// forfeit the game (game service handles game cleanup)
			let _: () = self
				.redis
				.xadd_maxlen(
					"game_requests",
					redis::streams::StreamMaxlen::Approx(10000),
					"*",
					&[(
						"forfeit",
						&serde_json::to_string(&(&game_id, &self.user_id))
							.expect("failed to serialize forfeit"),
					)],
				)
				.await
				.expect("redis error");
		}
		let _: usize = self
			.redis
			.del(&[
				format!("socket_state:{}", &self.user_id),
				format!("matchmaking:{}", &self.user_id),
				format!("user_socket_count:{}", &self.user_id),
				format!("disconnect_snowflake:{}", &self.user_id),
			])
			.await
			.expect("redis error");
	}
	async fn set_disconnect_snowflake(&mut self) -> String {
		let disconnect_snowflake = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
		let _: () = self
			.redis
			.set(
				format!("disconnect_snowflake:{}", &self.user_id),
				&disconnect_snowflake,
			)
			.await
			.expect("redis error");
		return disconnect_snowflake;
	}
	async fn get_disconnect_snowflake(&mut self) -> String {
		self.redis
			.get::<String, String>(format!("disconnect_snowflake:{}", &self.user_id))
			.await
			.expect("redis error")
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
				.xadd_maxlen(
					format!("matchmaking:{}", matched_player),
					redis::streams::StreamMaxlen::Approx(1000),
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
				.xadd_maxlen(
					"game_requests",
					redis::streams::StreamMaxlen::Approx(10000),
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
			self.matched(game_id).await;
		}
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
	pub async fn matched(&mut self, game_id: String) {
		self.state = PlaySocketState::Game {
			game_id,
			my_turn: false,
		};
		self.save_state().await;
	}
	pub async fn game_start(&mut self, game_start: String) {
		let game_start: GameStart =
			serde_json::from_str(&game_start).expect("failed to parse game start");
		self.state = PlaySocketState::Game {
			game_id: game_start.game_id,
			my_turn: false,
		};
		self.send_game_state().await;
		self.save_state().await;
	}
	pub async fn turn_start(&mut self, turn_start: String) {
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
	pub async fn handle_message(&mut self, message: &str) {
		let message: PlayRequest = match serde_json::from_str(message) {
			Ok(message) => message,
			Err(_) => return,
		};
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
				}
			}
			PlayRequest::ChatMessage { message } => {
				if message.len() > 1024 {
					return;
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
		}
	}
	pub async fn send_game_state(&mut self) {
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
