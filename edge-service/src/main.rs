mod play_socket;
mod util;

use play_socket::{PlaySocket, PlaySocketState};
use redis::streams::{StreamKey, StreamReadOptions, StreamReadReply};
use redis::{AsyncCommands, RedisFuture};
use rocket::futures::StreamExt;
use rocket::{Responder, Shutdown, get, launch, post, routes};

use crate::util::close_socket;
use rocket::http::{Cookie, CookieJar, SameSite};
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
) -> Result<Channel<'static>, ErrorResponse> {
	let user_id = match cookies.get_private("user_id") {
		Some(cookie) => cookie.value().to_string(),
		None => return Err(ErrorResponse::Unauthorized(())),
	};
	Ok(ws.channel(move |socket| {
		Box::pin(async move {
			let mut socket_state = match PlaySocket::new(socket, user_id, db, redis).await {
				Ok(s) => s,
				Err((msg, mut socket)) => {
					close_socket(&mut socket, msg).await;
					return Ok(());
				}
			};
			let stream_options = StreamReadOptions::default().block(1000).count(1);
			let mut redis = socket_state.redis.clone();
			let close_message;
			let allow_reconnect;
			let mut last_matchmaking_message_id: Option<String> = None;
			let mut last_game_message_id: Option<String> = None;
			'main_loop: loop {
				let last_id;
				let stream_key;
				let redis_stream: RedisFuture<StreamReadReply> = match &socket_state.state {
					PlaySocketState::Matchmaking { .. } => {
						stream_key = [format!("matchmaking:{}", socket_state.user_id)];
						last_id = [match &last_matchmaking_message_id {
							Some(id) => id.as_str(),
							None => "$",
						}];
						redis.xread_options(&stream_key, &last_id, &stream_options)
					}
					PlaySocketState::Game { game_id, .. } => {
						stream_key = [format!("game:{}", &game_id)];
						last_id = [match &last_game_message_id {
							Some(id) => id.as_str(),
							None => "$",
						}];
						redis.xread_options(&stream_key, &last_id, &stream_options)
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
					Ok(message) = redis_stream => {
						for StreamKey {ids, key} in message.keys {
							for message in ids {
								if key.starts_with("matchmaking:") {
									if let Some(game_id) = message.get::<String>("match") {
										socket_state.matched(game_id).await;
									}
									last_matchmaking_message_id = Some(message.id.clone());
								} else if key.starts_with("game:") {
									if let Some(game_start) = message.get::<String>("game_start") {
										socket_state.game_start(game_start).await;
									}
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
									last_game_message_id = Some(message.id.clone())
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
	}))
}

#[post("/login")]
async fn login(cookies: &CookieJar<'_>, mut db: Connection<PostgresPool>) {
	let id = match cookies.get_private("user_id") {
		Some(id) => id.value().to_string(),
		None => {
			let id = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
			sqlx::query("INSERT INTO users (id, elo) VALUES ($1, 1500) ON CONFLICT DO NOTHING")
				.bind(&id)
				.execute(&mut **db)
				.await
				.expect("postgres error");
			id.clone()
		}
	};
	cookies.add_private(
		Cookie::build(("user_id", id))
			.http_only(true)
			.permanent()
			.same_site(SameSite::Lax)
			.secure(true)
			.build(),
	);
}

#[derive(Responder)]
enum ErrorResponse {
	#[response(status = 401)]
	Unauthorized(()),
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
		.mount("/", routes![play, login])
		.attach(RedisPool::init())
		.attach(PostgresPool::init())
}
