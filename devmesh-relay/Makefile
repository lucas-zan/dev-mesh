.DEFAULT_GOAL := local
ENV_FILE ?= deploy/.env
REMOTE_HOST ?=
REMOTE_DIR ?= /opt/devmesh
PLATFORM ?=
RELAY_IMAGE ?=

.PHONY: local deploy status logs restart down config credentials help
local:
	python3 deploy/manage.py deploy --env-file "$(ENV_FILE)"
deploy:
	REMOTE_HOST="$(REMOTE_HOST)" REMOTE_DIR="$(REMOTE_DIR)" PLATFORM="$(PLATFORM)" RELAY_IMAGE="$(RELAY_IMAGE)" python3 deploy/remote.py
status logs restart down config credentials:
	python3 deploy/manage.py $@ --env-file "$(ENV_FILE)"
help:
	@printf '%s\n' 'make [local]  Build and start local PostgreSQL + Relay/Web on port 17843' 'make deploy REMOTE_HOST=user@server  Build locally, upload and deploy remotely' 'REMOTE_DIR=/opt/devmesh  Remote absolute deployment directory' 'PLATFORM=linux/amd64  Override detected remote architecture' 'make status    Show local containers' 'make logs      Follow local Relay logs' 'make restart   Restart local Relay' 'make down      Stop local containers; keep PostgreSQL data' 'make config    Validate local Compose configuration' 'make credentials  Show initial local credentials' 'ENV_FILE=path  Select local deployment environment file'
