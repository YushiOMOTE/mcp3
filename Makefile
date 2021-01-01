crate_name ?= mcp2
web_dir ?= $(PWD)/static/
wasm_dir ?= $(web_dir)/target


.PHONY: setup client server run-client run-server


setup:
	rustup target add wasm32-unknown-unknown
	cargo install wasm-bindgen-cli
	cargo install basic-http-server


client:
	cd client; \
		cargo build --target wasm32-unknown-unknown --release; \
		wasm-bindgen --out-dir $(wasm_dir) --target web target/wasm32-unknown-unknown/release/agarcli.wasm;


server:
	cd server; \
		cargo build --release


run-server: server
	cd server; \
		cargo run --release


run-client: client
	basic-http-server $(web_dir)
