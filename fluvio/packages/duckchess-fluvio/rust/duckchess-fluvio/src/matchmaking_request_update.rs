use crate::bindings::duckchess::duckchess_fluvio::types::FlMatchmakingRequest;
use crate::bindings::duckchess::duckchess_fluvio::types::*;
use sdfg::sdf;
use sdfg::Result;
#[sdf(
    fn_name = "matchmaking-request-update",
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
pub(crate) fn matchmaking_request_update(matchmaking_request: FlMatchmakingRequest) -> Result<()> {
	let mut state = matchmaking_state();
	match matchmaking_request {
		FlMatchmakingRequest::CancelMatchmaking(_cancel_matchmaking) => {
			// sdf doesn't currently allow deleting a row
			// state.delete();
		}
		FlMatchmakingRequest::ChangeEloRange(change_elo_range) => {
			state.elo_range = change_elo_range.elo_range;
		}
		FlMatchmakingRequest::StartMatchmaking(start_matchmaking) => {
			state.elo = start_matchmaking.elo;
			state.elo_range = start_matchmaking.elo_range;
			state.time_started = start_matchmaking.time_started;
		}
	};
	state.update()?;
	Ok(())
}
