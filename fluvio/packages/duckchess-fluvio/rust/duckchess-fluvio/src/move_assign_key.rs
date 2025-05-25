use crate::bindings::duckchess::duckchess_fluvio::types::FlMove;
use duckchess_types::Move;
use rocket::serde::json::serde_json;
use sdfg::anyhow;
use sdfg::sdf;
use sdfg::Result;
#[sdf(fn_name = "move-assign-key")]
pub(crate) fn move_assign_key(move_: FlMove) -> Result<u64> {
	let move_ =
		serde_json::from_str::<Move>(&move_).map_err(|_| anyhow::anyhow!("invalid move"))?;
	Ok(move_.game_id)
}
