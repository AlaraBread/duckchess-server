use crate::bindings::duckchess::duckchess_fluvio::types::FlMove;
use crate::bindings::duckchess::duckchess_fluvio::types::*;
use duckchess_types::Board;
use duckchess_types::Move;
use duckchess_types::MoveType;
use rocket::serde::json::serde_json;
use sdfg::sdf;
use sdfg::Result;

#[sdf(
    fn_name = "run-move",
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
pub(crate) fn run_move(move_: FlMove) -> Result<()> {
	let move_ = serde_json::from_str::<Move>(&move_)?;
	let mut board_state = board_state();
	let mut board = serde_json::from_str::<Board>(&board_state.board)?;
	if let MoveType::GameOver { .. } = move_.move_type {
		// sdf doesnt let you delete a row yet
		// board_state.delete();
	} else {
		board.do_move(&move_);
	}
	board_state.board = serde_json::to_string(&board)?;
	board_state.update()?;
	Ok(())
}
