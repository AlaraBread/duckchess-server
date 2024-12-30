use rocket::launch;

mod board;
mod cors;
mod game;
mod game_manager;
mod player_manager;

#[launch]
fn rocket() -> _ {
	rocket::build()
		.attach(player_manager::stage())
		.attach(game_manager::stage())
		.attach(game::stage())
		.attach(cors::stage())
}
