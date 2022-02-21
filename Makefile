RUSTFLAGS = RUSTFLAGS=--cfg=web_sys_unstable_apis

build:
	$(RUSTFLAGS) wasmbuild

release:
	$(RUSTFLAGS) wasmbuild --release
	wasm-opt -Oz lib/eszip_wasm_bg.wasm -o lib/eszip_wasm_bg.wasm

node:
	deno run -A ./build_npm.ts 0.0.0

test:
	deno test -A lib/

fmt:
	deno fmt lib/
	cargo fmt --all
