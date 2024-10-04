build:
	cargo build --manifest-path vhost/Cargo.toml
	cargo build --manifest-path vrouter/Cargo.toml
	cp snowcast-control/target/debug/vhost vhost
	cp snowcast-server/target/debug/vrouter vrouter

clean:
	cargo clean --manifest-path vhost/Cargo.toml
	cargo clean --manifest-path vrouter/Cargo.toml