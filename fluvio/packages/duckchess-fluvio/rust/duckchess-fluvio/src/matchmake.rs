use crate::bindings::duckchess::duckchess_fluvio::types::FlMatchmakingRequest;
use crate::bindings::duckchess::duckchess_fluvio::types::FlMatchmakingResponse;
use sdfg::sdf;
use sdfg::Result;
#[sdf(fn_name = "matchmake")]
pub(crate) fn matchmake(
	matchmaking_request: FlMatchmakingRequest,
) -> Result<Vec<FlMatchmakingResponse>> {
	let (player_id, elo, elo_range) = match matchmaking_request {
		FlMatchmakingRequest::CancelMatchmaking(_cancel_matchmaking) => return Ok(vec![]),
		FlMatchmakingRequest::ChangeEloRange(change_elo_range) => (
			change_elo_range.player_id,
			{
				let df = sql(&format!(
					"SELECT elo FROM matchmaking_state WHERE _key = {}",
					change_elo_range.player_id
				))?;
				df.rows()?.f32(&df.col("elo")?)?
			},
			change_elo_range.elo_range,
		),
		FlMatchmakingRequest::StartMatchmaking(start_matchmaking) => (
			start_matchmaking.player_id,
			start_matchmaking.elo,
			start_matchmaking.elo_range,
		),
	};
	let matches = sql(&format!(
		"SELECT * FROM matchmaking_state WHERE elo >= {} AND elo <= {} and _key != {} ORDER BY time_started DESC LIMIT 1",
		elo - elo_range,
		elo + elo_range,
		player_id
	))?;
	let opponent_id = matches.rows()?.u64(&matches.col("_key")?)?;
	Ok(vec![
		FlMatchmakingResponse {
			player_id,
			opponent_id,
		},
		FlMatchmakingResponse {
			player_id: opponent_id,
			opponent_id: player_id,
		},
	])
}
