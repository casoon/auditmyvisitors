.PHONY: build release check clean

build:
	. ./.env.local && cargo build

release:
	. ./.env.local && cargo build --release

check:
	cargo check

clean:
	cargo clean
