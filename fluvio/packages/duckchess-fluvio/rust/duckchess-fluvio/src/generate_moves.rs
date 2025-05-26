use crate::bindings::duckchess::duckchess_fluvio::types::FlMove;
use crate::bindings::duckchess::duckchess_fluvio::types::FlTurn;
use crate::bindings::duckchess::duckchess_fluvio::types::*;
use duckchess_types::Board;
use rocket::serde::json::serde_json;
use sdfg::anyhow;
use sdfg::context_guest::wit::utils::sql;
use sdfg::sdf;
use sdfg::Result;
#[sdf(
	fn_name = "generate-moves",
	state = (
		name = "board-state",
		ty = row,
		update_fn = {
			use sdfg::row_guest::bindings::sdf::row_state::types::Dvalue;
			self.resource.set(
				&[("board".to_string(), Dvalue::String(self.board.clone())),
				]
		).map_err(|e|sdfg::anyhow::anyhow!("Failed to update row: {}", e))?;},
	),
)]
pub(crate) fn generate_moves(turn: FlTurn) -> Result<Vec<FlMove>> {
	// sdf doesn't have proper sql parameter support
	// this is fine because turn.game_id will only ever be an integer
	let board_df = sql(&format!(
		"select board from board_state where _key = {}",
		turn.game_id
	))?;
	let rows = board_df.rows()?;
	let board_col = board_df.col("board")?;
	if !rows.next() {
		return Err(anyhow::anyhow!("non existent board"));
	}
	let board = serde_json::from_str::<Board>(&rows.str(&board_col)?)?;
	if let Some(moves) = board.evaluate_turn(
		turn.game_id,
		turn.piece_idx as usize,
		turn.move_idx as usize,
	) {
		moves
			.into_iter()
			.map(|m| serde_json::to_string(&m))
			.collect::<Result<Vec<String>, _>>()
			.map_err(|_| anyhow::anyhow!("invalid move"))
	} else {
		Err(anyhow::anyhow!("invalid turn"))
	}
}
