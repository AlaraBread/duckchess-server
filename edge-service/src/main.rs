mod play_socket;
mod util;

use play_socket::{PlaySocket, PlaySocketState};
use redis::streams::{StreamKey, StreamReadOptions, StreamReadReply};
use redis::{AsyncCommands, RedisFuture};
use rocket::futures::StreamExt;
use rocket::{Shutdown, get, launch, routes};

use crate::util::{close_socket, conditional_future};
use rocket::http::{Cookie, CookieJar};
use rocket::tokio;
use rocket_db_pools::{
	Connection, Database,
	deadpool_redis::{self},
	sqlx,
};
use uuid::{NoContext, Timestamp, Uuid};
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
			let allow_reconnect;
			'main_loop: loop {
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
								allow_reconnect = true;
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
								if let Some(winner) = message.get::<String>("end") {
									socket_state.game_end(winner).await;
									close_message = "game ended";
									allow_reconnect = false;
									break 'main_loop;
								}
							}
						}
					}
					_ = &mut end => {
						close_message = "server closed";
						allow_reconnect = true;
						break;
					}
					else => {
						close_message = "client disconnected";
						allow_reconnect = true;
						break;
					}
				}
			}
			socket_state
				.disconnected(&close_message, allow_reconnect)
				.await;
			Ok(())
		})
	})
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
