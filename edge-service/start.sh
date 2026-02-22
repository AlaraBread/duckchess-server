#!/bin/sh

echo "[release]" > Rocket.toml
if [ -n "${CORS_ALLOWED_ORIGINS}" ]; then
	echo "cors_allowed_origins = ${CORS_ALLOWED_ORIGINS}" >> Rocket.toml
fi
if [ -n "${CORS_ALLOW_ALL_ORIGINS}" ]; then
	echo "cors_allow_all_origins = ${CORS_ALLOW_ALL_ORIGINS}" >> Rocket.toml
fi
if [ -n "${COOKIES_SAME_SITE}" ]; then
	echo "cookies_same_site = \"${COOKIES_SAME_SITE}\"" >> Rocket.toml
fi
if [ -n "${LOG_LEVEL}" ]; then
	echo "log_level = \"${LOG_LEVEL}\"" >> Rocket.toml
fi
if [ -n "${BIND_ADDRESS}" ]; then
	echo "address = \"${BIND_ADDRESS}\"" >> Rocket.toml
fi
if [ -n "${SERVE_PORT}" ]; then
	echo "port = ${SERVE_PORT}" >> Rocket.toml
fi
if [ -n "${SECRET_KEY_FILE}" ]; then
	echo "secret_key = \"$(cat $SECRET_KEY_FILE)\"" >> Rocket.toml
fi
echo "[default.databases.postgres]" >> Rocket.toml
echo "url = \"postgres://${POSTGRES_USER}:$(cat $POSTGRES_PASSWORD_FILE)@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}\"" >> Rocket.toml
echo "[default.databases.redis]" >> Rocket.toml
echo "url = \"${REDIS_URL}\"" >> Rocket.toml

exec "${BINARY_PATH}"
