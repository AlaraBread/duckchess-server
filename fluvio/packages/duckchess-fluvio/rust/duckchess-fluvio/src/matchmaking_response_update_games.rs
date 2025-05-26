use crate::bindings::duckchess::duckchess_fluvio::types::FlMatchmakingResponse;
use crate::bindings::duckchess::duckchess_fluvio::types::*;
use duckchess_types::Board;
use rand::Rng;
use rocket::serde::json::serde_json;
use sdfg::sdf;
use sdfg::Result;

#[sdf(
    fn_name = "matchmaking-response-update-games",
    state = (
        name = "board-state",
        ty = row,
        update_fn = {use
        sdfg::row_guest::bindings::sdf::row_state::types::Dvalue;self.resource.set(
            &[("board".to_string(), Dvalue::String(self.board.clone())),
            ]
        ).map_err(|e|sdfg::anyhow::anyhow!("Failed to update row: {}", e))?;},
    ),
)]
pub(crate) fn matchmaking_response_update_games(
	matchmaking_response: FlMatchmakingResponse,
) -> Result<()> {
	if matchmaking_response.player_id >= matchmaking_response.opponent_id {
		// matchmaking responses get sent in pairs
		// we only need to make a game for one response
		// so we arbitrarily pick one from the pair
		return Ok(());
	}
	if matchmaking_response.existing {
		return Ok(());
	}
	let mut board_state = board_state();
	let (white, black) = if rand::rng().random() {
		(
			matchmaking_response.player_id,
			matchmaking_response.opponent_id,
		)
	} else {
		(
			matchmaking_response.opponent_id,
			matchmaking_response.player_id,
		)
	};
	// using white's player id as the game id for now
	let board = Board::new(white, black, white);
	board_state.board = serde_json::to_string(&board)?;
	board_state.update()?;
	Ok(())
}
