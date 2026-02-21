#!/bin/sh

echo "[release]" > Rocket.toml && \
	echo "log_level = \"${LOG_LEVEL}\"" >> Rocket.toml && \
	echo "address = \"${BIND_ADDRESS}\"" >> Rocket.toml && \
	echo "port = ${SERVE_PORT}" >> Rocket.toml && \
    echo "secret_key = \"$(cat $SECRET_KEY_FILE)\"" >> Rocket.toml && \
	echo "[default.databases.postgres]" >> Rocket.toml && \
	echo "url = \"postgres://${POSTGRES_USER}:$(cat $POSTGRES_PASSWORD_FILE)@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}\"" >> Rocket.toml && \
	echo "[default.databases.redis]" >> Rocket.toml && \
	echo "url = \"${REDIS_URL}\"" >> Rocket.toml

exec "${BINARY_PATH}"
