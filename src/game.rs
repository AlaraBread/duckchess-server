use rocket::{
	fairing::AdHoc,
	get,
	http::CookieJar,
	post,
	response::{
		stream::{Event, EventStream},
		Responder,
	},
	routes,
	serde::{json::Json, Deserialize, Serialize},
	tokio::{
		select,
		sync::broadcast::{channel, error::RecvError, Sender},
	},
	Shutdown, State,
};

use crate::{board::Game, game_manager::GameManager, player_manager::PlayerManager};

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game", |rocket| async {
		rocket
			.manage(channel::<ListenMessage>(1024).0)
			.mount("/game", routes![listen, turn, find_match, get_game_state])
	})
}

#[post("/find_match")]
async fn find_match(
	cookies: &CookieJar<'_>,
	game_manager: &State<GameManager>,
	player_manager: &State<PlayerManager>,
	queue: &State<Sender<ListenMessage>>,
) -> Json<u64> {
	let player_id = player_manager.get_player_id(cookies);
	let game_id = game_manager.find_match(player_id).await;
	let _ = queue.send(ListenMessage {
		game_id,
		player_id: None,
		response: ListenResponse::PlayerJoined { id: player_id },
	});
	return Json(game_id);
}

#[post("/<game_id>/turn")]
async fn turn(
	game_id: u64,
	queue: &State<Sender<ListenMessage>>,
	game_manager: &State<GameManager>,
	player_manager: &State<PlayerManager>,
	cookies: &CookieJar<'_>,
) -> Result<(), ErrorResponse> {
	let game = game_manager.get_game(game_id).await;
	match game {
		Some(game) => {
			if !game.has_player(player_manager.get_player_id(cookies)) {
				return Err(ErrorResponse::Forbidden(()));
			}
			let _ = queue.send(ListenMessage {
				game_id,
				player_id: None,
				response: ListenResponse::Turn {},
			});
			return Ok(());
		}
		None => {
			return Err(ErrorResponse::NotFound(()));
		}
	};
}

#[get("/<game_id>")]
async fn get_game_state(
	game_id: u64,
	game_manager: &State<GameManager>,
	cookies: &CookieJar<'_>,
	player_manager: &State<PlayerManager>,
) -> Result<Json<Game>, ErrorResponse> {
	let game = game_manager.get_game(game_id).await;
	match game {
		Some(game) => {
			if !game.has_player(player_manager.get_player_id(cookies)) {
				return Err(ErrorResponse::Forbidden(()));
			}
			return Ok(Json(game));
		}
		None => {
			return Err(ErrorResponse::NotFound(()));
		}
	};
}

#[derive(Clone, Debug)]
struct ListenMessage {
	game_id: u64,
	player_id: Option<u64>,
	response: ListenResponse,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "rocket::serde", rename_all = "camelCase", tag = "type")]
enum ListenResponse {
	PlayerJoined { id: u64 },
	Turn,
}

#[get("/<game_id>/listen")]
async fn listen(
	game_id: u64,
	queue: &State<Sender<ListenMessage>>,
	mut end: Shutdown,
	cookies: &CookieJar<'_>,
	game_manager: &State<GameManager>,
	player_manager: &State<PlayerManager>,
) -> Result<EventStream![], ErrorResponse> {
	let player_id = player_manager.get_player_id(cookies);
	match game_manager.get_game(game_id).await {
		Some(game) => {
			if !game.has_player(player_id) {
				return Err(ErrorResponse::Forbidden(()));
			}
		}
		None => return Err(ErrorResponse::NotFound(())),
	}
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
			if msg.game_id != game_id {
				continue;
			}
			yield Event::json(&msg.response);
		}
	})
}

#[derive(Responder)]
enum ErrorResponse {
	#[response(status = 400)]
	Forbidden(()),
	#[response(status = 404)]
	NotFound(()),
}
