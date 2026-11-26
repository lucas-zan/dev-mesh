.PHONY: build test format lint verify bundle install configure deploy relay-deploy relay-local
build:
	cargo build --workspace --offline
	npm run build --workspaces --if-present
test:
	cargo test --workspace --offline
	npm test --workspaces --if-present
	python3 scripts/test_skill.py
format:
	cargo fmt --all
lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --offline -- -D warnings
verify:
	python3 scripts/verify.py
bundle:
	python3 scripts/package.py --output .cache/release/devmesh-0.1.0
install:
	python3 scripts/install_local.py $(INSTALL_ARGS)
configure:
	python3 scripts/setup_local.py $(CONFIGURE_ARGS)
deploy relay-deploy:
	$(MAKE) -C devmesh-relay deploy
relay-local:
	$(MAKE) -C devmesh-relay local
