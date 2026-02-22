mod config;
mod play_socket;
mod util;

use crate::config::CustomConfig;
use crate::util::close_socket;
use play_socket::{PlaySocket, PlaySocketState};
use redis::streams::{StreamKey, StreamReadOptions, StreamReadReply};
use redis::{AsyncCommands, RedisFuture};
use rocket::fairing::AdHoc;
use rocket::futures::StreamExt;
use rocket::http::Method;
use rocket::http::{Cookie, CookieJar, SameSite};
use rocket::serde::json::Json;
use rocket::{Responder, Shutdown, get, routes};
use rocket::{State, tokio};
use rocket_cors::{AllowedHeaders, AllowedOrigins};
use rocket_db_pools::{
	Connection, Database,
	deadpool_redis::{self},
	sqlx,
};
use std::error::Error;
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
				Err((msg, socket)) => {
					close_socket(socket, msg).await;
					return Ok(());
				}
			};
			let stream_options = StreamReadOptions::default().block(1000).count(1);
			let mut redis = socket_state.redis.clone();
			let close_message;
			let allow_reconnect;
			let surrender;
			'main_loop: loop {
				let last_id;
				let stream_key;
				let redis_stream: RedisFuture<StreamReadReply> = match &socket_state.state {
					PlaySocketState::Game {
						game_id,
						last_message,
						..
					} => {
						stream_key = [format!("game:{}", &game_id)];
						last_id = [match &last_message {
							Some(id) => id.as_str(),
							None => "0-0",
						}];
						redis.xread_options(&stream_key, &last_id, &stream_options)
					}
					PlaySocketState::Matchmaking { last_message, .. }
					| PlaySocketState::WaitingForSetup { last_message, .. } => {
						stream_key = [format!("user:{}", socket_state.user_id)];
						last_id = [match &last_message {
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
								if let Some(msg) = socket_state.handle_message(&text).await {
									close_message = msg;
									allow_reconnect = false;
									surrender = true;
									break;
								}
							}
							ws::Message::Close(_) => {
								close_message = "client disconnected";
								allow_reconnect = true;
								surrender = true;
								break;
							}
							_ => {}
						}
					}
					Ok(message) = redis_stream => {
						for StreamKey {ids, ..} in message.keys {
							for message in ids {
								if let Some(msg) = socket_state.process_stream_id(message).await {
									close_message = msg;
									allow_reconnect = false;
									surrender = false;
									break 'main_loop;
								}
							}
						}
						socket_state.save_state().await;
					}
					_ = &mut end => {
						close_message = "server closed";
						allow_reconnect = true;
						surrender = false;
						break;
					}
					else => {
						close_message = "client disconnected";
						allow_reconnect = true;
						surrender = true;
						break;
					}
				}
				if let Some(msg) = socket_state.tick().await {
					close_message = msg;
					allow_reconnect = false;
					surrender = true;
					break;
				}
			}
			socket_state
				.disconnected(&close_message, allow_reconnect, surrender)
				.await;
			Ok(())
		})
	}))
}

#[get("/login")]
async fn login(
	cookies: &CookieJar<'_>,
	mut db: Connection<PostgresPool>,
	config: &State<CustomConfig>,
) -> Json<String> {
	let id;
	let id = match cookies.get_private("user_id") {
		Some(id) => id.value().to_string(),
		None => {
			id = Uuid::new_v7(Timestamp::now(NoContext)).to_string();
			sqlx::query("INSERT INTO users (id, elo) VALUES ($1, 1500) ON CONFLICT DO NOTHING")
				.bind(&id)
				.execute(&mut **db)
				.await
				.expect("postgres error");
			id
		}
	};
	cookies.add_private(
		Cookie::build(("user_id", id.clone()))
			.http_only(true)
			.permanent()
			.same_site(SameSite::from(&config.cookies_same_site))
			.secure(true)
			.build(),
	);
	Json(id)
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

#[rocket::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let partial_rocket = rocket::build()
		.mount("/", routes![play, login])
		.attach(AdHoc::config::<CustomConfig>());
	let custom_config = partial_rocket.figment().extract::<CustomConfig>().unwrap();
	let allowed_origins = if custom_config.cors_allow_all_origins {
		AllowedOrigins::all()
	} else {
		AllowedOrigins::some_exact(&custom_config.cors_allowed_origins)
	};
	let cors = rocket_cors::CorsOptions {
		allowed_origins,
		allowed_methods: vec![Method::Get].into_iter().map(From::from).collect(),
		allowed_headers: AllowedHeaders::some(&["Authorization", "Accept"]),
		allow_credentials: true,
		..Default::default()
	}
	.to_cors()?;
	Ok(partial_rocket
		.attach(cors)
		.attach(RedisPool::init())
		.attach(PostgresPool::init())
		.launch()
		.await
		.map(|_| ())?)
}
