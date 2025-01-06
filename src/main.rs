use rocket::launch;

mod board;
mod broadcast_manager;
mod cors;
mod game;
mod game_manager;
mod piece;
mod play;
mod player_manager;
mod vec2;

#[launch]
fn rocket() -> _ {
	rocket::build()
		.attach(player_manager::stage())
		.attach(broadcast_manager::stage())
		.attach(game_manager::stage())
		.attach(play::stage())
		.attach(cors::stage())
}
