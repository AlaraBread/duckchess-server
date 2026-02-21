#!/bin/sh

echo "REDIS_URL=${REDIS_URL}" > .env && \
	echo "AUTOCLAIM_TIME_MS=${AUTOCLAIM_TIME_MS}" >> .env && \
	echo "CONSUMER_ID=${CONSUMER_ID}" >> .env && \
	echo "CONSUMER_GROUP=${CONSUMER_GROUP}" >> .env

exec "${BINARY_PATH}"
