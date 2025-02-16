use std::sync::atomic::{AtomicU64, Ordering};

use rocket::{
	fairing::AdHoc,
	http::{Cookie, CookieJar, SameSite},
};

pub fn stage() -> AdHoc {
	AdHoc::on_ignite("game manager", |rocket| async {
		rocket.manage(PlayerManager::default())
	})
}

#[derive(Default)]
pub struct PlayerManager {
	player_counter: AtomicU64,
}

impl PlayerManager {
	pub fn get_player_id(&self, cookies: &CookieJar<'_>) -> u64 {
		cookies
			.get("playerId")
			.and_then(|cookie| Some(cookie.value()))
			.and_then(|value| match value.parse() {
				Ok(id) => Some(id),
				Err(_) => None,
			})
			.unwrap_or_else(|| {
				let id = self.player_counter.fetch_add(1, Ordering::Relaxed) + 1;
				cookies.add(
					Cookie::build(("playerId", id.to_string()))
						.permanent()
						.same_site(SameSite::Strict)
						.build(),
				);
				return id;
			})
	}
}
