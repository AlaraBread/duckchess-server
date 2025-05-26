use crate::bindings::duckchess::duckchess_fluvio::types::FlTurn;
use crate::bindings::duckchess::duckchess_fluvio::types::*;
use duckchess_types::Board;
use rocket::serde::json::serde_json;
use sdfg::sdf;
use sdfg::Result;
#[sdf(
    fn_name = "generate-responses",
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
pub(crate) fn generate_responses(_turn: FlTurn) -> Result<()> {
	let mut board_state = board_state();
	let mut board = serde_json::from_str::<Board>(&board_state.board)?;
	board.generate_moves(true);
	board_state.board = serde_json::to_string(&board)?;
	board_state.update()?;
	Ok(())
}
