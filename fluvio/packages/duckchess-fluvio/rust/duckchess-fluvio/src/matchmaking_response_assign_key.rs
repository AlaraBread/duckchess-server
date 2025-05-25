use crate::bindings::duckchess::duckchess_fluvio::types::FlMatchmakingResponse;
use sdfg::sdf;
use sdfg::Result;
#[sdf(fn_name = "matchmaking-response-assign-key")]
pub(crate) fn matchmaking_response_assign_key(
	matchmaking_response: FlMatchmakingResponse,
) -> Result<u64> {
	Ok(matchmaking_response.player_id)
}
