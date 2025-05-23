use crate::bindings::duckchess::matchmaking::types::FlMatchmakingRequest;
#[allow(unused_imports)]
use crate::bindings::duckchess::matchmaking::types::*;
use sdfg::sdf;
use sdfg::Result;
#[sdf(fn_name = "matchmaking-request-assign-key")]
pub(crate) fn matchmaking_request_assign_key(
	matchmaking_request: FlMatchmakingRequest,
) -> Result<u64> {
	Ok(match matchmaking_request {
		FlMatchmakingRequest::CancelMatchmaking(cancel_matchmaking) => cancel_matchmaking.player_id,
		FlMatchmakingRequest::ChangeEloRange(change_elo_range) => change_elo_range.player_id,
		FlMatchmakingRequest::StartMatchmaking(start_matchmaking) => start_matchmaking.player_id,
	})
}
