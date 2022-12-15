#!/usr/bin/env bash
export PGDATA="$(dirname $(readlink -f "$0"))/data"
if [[ ! -d "$PGDATA" ]]; then
	pg_ctl init
fi

if [[ "$1" == "start" ]]; then
	mkdir -p "$TMP/postgresql"
	pg_ctl \
		-o "-c unix_socket_directories=$TMP/postgresql" \
		start
else
	pg_ctl "$@"
fi
