# duckchess game server

This is a websocket server that hosts duckchess, an experimental chesslike game.

The game itself is still a work in progress, but most of the technical details are in place.

The server is fully authoritative and the client doesn't need to know anything about how the pieces move.

# TODO
- Make a seperate service that does authentication.
- Setup load balancing.
  - I dont think a single game server could handle that many concurrent games.
  - Need to do performance testing to determine how nessecary this is.
