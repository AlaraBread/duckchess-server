CREATE TABLE users (
	id CHAR(36) NOT NULL PRIMARY KEY,
	elo REAL NOT NULL DEFAULT 1500
);

CREATE TABLE matchmaking_players (
	id CHAR(36) NOT NULL PRIMARY KEY,
	elo REAL NOT NULL DEFAULT 1500,
	elo_range REAL NOT NULL DEFAULT 1500,
	start_time TIMESTAMP NOT NULL DEFAULT NOW(),
	board_setup TEXT NOT NULL
);
