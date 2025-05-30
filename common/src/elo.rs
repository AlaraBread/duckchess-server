pub fn update_rating(elo: f32, opponent_elo: f32, result: f32) -> (f32, f32) {
	let expected = 1.0 / (1.0 + 10.0.powf((opponent_elo - elo) / 400.0));
	let change = 32.0 * (result - expected);
	(elo + change, opponent_elo - change)
}
