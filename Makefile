RUSTFLAGS = RUSTFLAGS=--cfg=web_sys_unstable_apis

build:
	$(RUSTFLAGS) wasmbuild

release:
	$(RUSTFLAGS) wasmbuild --release

test:
	deno test -A lib/

fmt:
	deno fmt lib/
	cargo fmt --all

