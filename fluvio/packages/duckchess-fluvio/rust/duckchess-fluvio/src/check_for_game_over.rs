use crate::bindings::duckchess::duckchess_fluvio::types::FlMove;
use crate::bindings::duckchess::duckchess_fluvio::types::FlTurn;
use crate::bindings::duckchess::duckchess_fluvio::types::*;
use duckchess_types::Board;
use duckchess_types::Move;
use duckchess_types::MoveType;
use duckchess_types::Vec2;
use rocket::serde::json::serde_json;
use sdfg::Result;
use sdfg::anyhow;

use sdfg::sdf;

#[sdf(
    fn_name = "check-for-game-over",
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
pub(crate) fn check_for_game_over(turn: FlTurn) -> Result<Vec<FlMove>> {
	// sdf doesn't have proper sql parameter support
	// this is fine because turn.game_id will only ever be an integer
	let board_df = sql(&format!(
		"select board from board_state where _key = {}",
		turn.game_id
	))?;
	let rows = board_df.rows()?;
	if !rows.next() {
		return Err(anyhow::anyhow!("non existent board"));
	}
	let board = serde_json::from_str::<Board>(&rows.str(&board_df.col("board")?)?)?;
	if board.moves.len() == 0 {
		Ok(vec![serde_json::to_string(&Move {
			game_id: turn.game_id,
			move_type: MoveType::GameOver {
				winner: !board.turn,
			},
			from: Vec2(-1, -1),
			to: Vec2(-1, -1),
		})?])
	} else {
		Ok(vec![])
	}
}
