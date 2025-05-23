use crate::bindings::duckchess::matchmaking::types::FlMatchmakingResponse;
#[allow(unused_imports)]
use crate::bindings::duckchess::matchmaking::types::*;
use sdfg::sdf;
use sdfg::Result;
#[sdf(
    fn_name = "matchmaking-response-update",
    state = (
        name = "matchmaking-state",
        ty = row,
        update_fn = {use
        sdfg::row_guest::bindings::sdf::row_state::types::Dvalue;self.resource.set(
            &[("elo".to_string(), Dvalue::Float32(self.elo.clone())),
            ("elo-range".to_string(), Dvalue::Float32(self.elo_range.clone())),
            ("time-started".to_string(), Dvalue::U64(self.time_started.clone())),
            ]
        ).map_err(|e|sdfg::anyhow::anyhow!("Failed to update row: {}", e))?;},
    ),
)]
pub(crate) fn matchmaking_response_update(
	matchmaking_response: FlMatchmakingResponse,
) -> Result<()> {
	let state = matchmaking_state();
	// sdf doesn't currently allow deleting a row
	// state.delete();
	Ok(())
}
