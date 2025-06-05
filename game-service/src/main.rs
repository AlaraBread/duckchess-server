use dotenvy::dotenv;
use duckchess_common::{Board, ChatMessage, GameStart, Player, Turn, TurnStart};
use redis::{
	AsyncCommands,
	aio::MultiplexedConnection,
	streams::{
		StreamAutoClaimOptions, StreamAutoClaimReply, StreamId, StreamKey, StreamReadOptions,
		StreamReadReply,
	},
};
use rocket::serde::json::serde_json;
use std::fmt::Debug;
use std::{
	any::type_name,
	collections::HashMap,
	env,
	str::FromStr,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
};

#[tokio::main]
async fn main() {
	dotenv().expect("failed to load .env");
	let env_vars = env::vars().collect::<HashMap<String, String>>();
	let redis_url: String = get_env_var(&env_vars, "REDIS_URL");
	let autoclaim_time: u64 = get_env_var(&env_vars, "AUTOCLAIM_TIME_MS");
	let consumer_id: String = get_env_var(&env_vars, "CONSUMER_ID");
	let consumer_group: String = get_env_var(&env_vars, "CONSUMER_GROUP");

	let should_exit = Arc::new(AtomicBool::new(false));
	let should_exit_2 = should_exit.clone();
	ctrlc::set_handler(move || {
		println!("received Ctrl+C. gracefully shutting down.");
		should_exit_2.store(true, Ordering::Relaxed);
	})
	.expect("Error setting Ctrl-C handler");

	let client =
		redis::Client::open(format!("redis://{}", redis_url)).expect("failed to open redis client");
	let mut con = client
		.get_multiplexed_async_connection()
		.await
		.expect("couldnt connect to redis");

	'start: loop {
		// Create consumer group if it doesn't exist
		let _create_group_result: Result<(), redis::RedisError> = con
			.xgroup_create_mkstream("game_requests", &consumer_group, "$")
			.await;

		loop {
			// autoclaim unacked messages
			let mut last_claimed_message: Option<String> = None;
			loop {
				if let Ok(autoclaim_result) = con
					.xautoclaim_options::<&str, &str, &str, u64, &str, StreamAutoClaimReply>(
						"game_requests",
						&consumer_group,
						&consumer_id,
						autoclaim_time,
						match &last_claimed_message {
							Some(id) => id.as_str(),
							None => "0-0",
						},
						StreamAutoClaimOptions::default(),
					)
					.await
				{
					for stream_id in autoclaim_result.claimed.iter() {
						process_stream_id(&mut con, stream_id).await;
					}
					ack_messages(&mut con, &consumer_group, autoclaim_result.claimed).await;
					last_claimed_message = Some(autoclaim_result.next_stream_id.clone());
					if autoclaim_result.next_stream_id == "0-0" {
						break;
					}
				} else {
					println!("game requests doesnt exist");
					continue 'start;
				}
			}
			// read new messages
			if let Ok(StreamReadReply { keys }) = con
				.xread_options(
					&["game_requests"],
					&[">"],
					&StreamReadOptions::default()
						.count(100)
						.block(1000)
						.group(&consumer_group, "todo"),
				)
				.await
			{
				for StreamKey { ids, .. } in keys.iter() {
					for stream_id in ids.iter() {
						process_stream_id(&mut con, stream_id).await;
					}
				}
				ack_messages(
					&mut con,
					&consumer_group,
					keys.into_iter()
						.flat_map(|StreamKey { ids, .. }| ids.into_iter())
						.collect(),
				)
				.await;
			} else {
				println!("game requests doesnt exist");
				continue 'start;
			}
			if should_exit.load(Ordering::Relaxed) {
				break 'start;
			}
		}
	}
}

async fn ack_messages(con: &mut MultiplexedConnection, consumer_group: &str, keys: Vec<StreamId>) {
	if keys.is_empty() {
		return;
	}
	let _: u64 = con
		.xack(
			"game_requests",
			&[consumer_group],
			&keys
				.into_iter()
				.map(|StreamId { id, .. }| id)
				.collect::<Vec<String>>(),
		)
		.await
		.expect("failed to ack messages");
}

fn get_env_var<T>(env_vars: &HashMap<String, String>, name: &str) -> T
where
	T: FromStr,
	T::Err: Debug,
{
	let value = env_vars
		.get(name)
		.expect(format!("{} not set", name).as_str());
	value.parse::<T>().expect(&format!(
		"{} = {} is not a {}",
		name,
		value,
		type_name::<T>()
	))
}

async fn process_stream_id(con: &mut MultiplexedConnection, stream_id: &StreamId) {
	if let Some(game_id) = stream_id.get::<String>("game_start") {
		process_game_start(con, game_id.as_str()).await;
	}
	if let Some(turn) = stream_id.get::<String>("turn") {
		process_turn(con, turn.as_str()).await;
	}
	if let Some(forfeit) = stream_id.get::<String>("forfeit") {
		process_forfeit(
			con,
			serde_json::from_str(&forfeit).expect("failed to parse forfeit"),
		)
		.await;
	}
}

async fn process_turn(con: &mut MultiplexedConnection, turn: &str) {
	let turn: Turn = serde_json::from_str(turn).expect("failed to parse turn");
	let board_key = format!("board:{}", turn.game_id);
	let board_str: String = con.get(&board_key).await.expect("failed to get board");
	let mut board: Board = serde_json::from_str(board_str.as_str()).expect("failed to parse board");
	let (computed_moves, game_over) = match board.evaluate_turn(&turn) {
		Some(o) => o,
		None => return,
	};
	let _: () = con
		.set(&board_key, serde_json::to_string(&board).unwrap())
		.await
		.expect("failed to set board");
	let _: String = con
		.xadd_maxlen(
			format!("game:{}", turn.game_id),
			redis::streams::StreamMaxlen::Approx(1000),
			"*",
			&[
				("moves", serde_json::to_string(&computed_moves).unwrap()),
				(
					"turn_start",
					serde_json::to_string(&TurnStart {
						turn: board.turn,
						move_pieces: board.move_pieces.clone(),
						moves: board.moves.clone(),
					})
					.expect("failed to serialize turn start"),
				),
			],
		)
		.await
		.expect("Failed to write to moves stream");
	if game_over {
		end_game(con, &board, &board.get_not_turn_player_id()).await;
	}
}

async fn process_game_start(con: &mut MultiplexedConnection, game_start_str: &str) {
	let game_start: GameStart =
		serde_json::from_str(game_start_str).expect("failed to parse game start");
	let board_key = format!("board:{}", game_start.game_id);
	let game_id = game_start.game_id.clone();
	let white_id = game_start.white.id.clone();
	let black_id = game_start.black.id.clone();
	let board = Board::new(game_start);
	let _: () = con
		.set(
			&board_key,
			serde_json::to_string(&board).expect("failed to serialize board"),
		)
		.await
		.expect("failed to set board");
	let message = [
		("game_start", game_start_str),
		(
			"turn_start",
			&serde_json::to_string(&TurnStart {
				turn: Player::White,
				move_pieces: board.move_pieces.clone(),
				moves: board.moves.clone(),
			})
			.expect("failed to serialize turn start"),
		),
	];
	let _: String = con
		.xadd_maxlen(
			format!("game:{}", &game_id),
			redis::streams::StreamMaxlen::Approx(1000),
			"*",
			&message,
		)
		.await
		.expect("failed to write to game stream");
	let _: String = con
		.xadd_maxlen(
			format!("user:{}", &black_id),
			redis::streams::StreamMaxlen::Approx(1000),
			"*",
			&message,
		)
		.await
		.expect("failed to write to user stream");
	let _: String = con
		.xadd_maxlen(
			format!("user:{}", &white_id),
			redis::streams::StreamMaxlen::Approx(1000),
			"*",
			&message,
		)
		.await
		.expect("failed to write to user stream");
	if board.moves.is_empty() {
		end_game(con, &board, &board.white_player).await;
	}
}

async fn process_forfeit(con: &mut MultiplexedConnection, (game_id, player_id): (String, String)) {
	let board_key = format!("board:{}", game_id);
	let board_str: String = match con.get(&board_key).await {
		Ok(board_str) => board_str,
		Err(_) => return,
	};
	let board: Board = serde_json::from_str(board_str.as_str()).expect("failed to parse board");
	let winner = if board.white_player == player_id {
		board.black_player.clone()
	} else {
		board.white_player.clone()
	};
	end_game(con, &board, &winner).await;
}

async fn end_game(con: &mut MultiplexedConnection, board: &Board, winner: &str) {
	let chat_message = ChatMessage {
		id: "".to_string(),
		message: format!(
			"{} wins",
			if board.white_player == winner {
				"white"
			} else {
				"black"
			}
		),
	};
	let message = serde_json::to_string(&chat_message).expect("failed to serialize chat message");
	let _: () = con
		.xadd_maxlen(
			format!("game:{}", board.id),
			redis::streams::StreamMaxlen::Approx(1000),
			"*",
			&[("chat", message.as_str()), ("end", winner)],
		)
		.await
		.expect("failed to write to game stream");
	for key in [
		format!("board:{}", board.id),
		format!("game:{}", board.id),
		format!("chat:{}", board.id),
	] {
		let _: i32 = con.expire(key, 30).await.expect("failed to expire key");
	}
}
