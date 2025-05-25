use crate::bindings::duckchess::duckchess_fluvio::types::FlTurn;
use sdfg::sdf;
use sdfg::Result;
#[sdf(fn_name = "turn-assign-key")]
pub(crate) fn turn_assign_key(turn: FlTurn) -> Result<u64> {
	Ok(turn.game_id)
}
