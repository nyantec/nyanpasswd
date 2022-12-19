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
elif [[ "$1" == "migrate" ]]; then
	psql -h "$TMP/postgresql" postgres <<EOF
CREATE DATABASE mail;
\connect mail
\i migrations/0001_init.sql
EOF
elif [[ "$1" == "sql" || "$1" == "psql" ]]; then
	psql -h "$TMP/postgresql" mail
else
	pg_ctl "$@"
fi
