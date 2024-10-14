ROOT_DIR := $(shell pwd)

LIBRARY_DIR := ip-the-better-tech-house-group/ip-imp/library
VNODE_DIR := ip-the-better-tech-house-group/ip-imp/vnode

VHOST_OUT := $(ROOT_DIR)/vhost
VROUTER_OUT := $(ROOT_DIR)/vrouter

debug: build_debug

build_debug:
	cargo build --manifest-path ./ip-imp/cargo.toml
	cp vnode/target/debug/vnode $(VHOST_OUT)
	cp vnode/target/debug/vnode vrouter $(VROUTER_OUT)

build:
	cargo build --manifest-path ./ip-imp/cargo.toml --release
	cp vnode/target/release/vnode $(VHOST_OUT)
	cp vnode/target/release/vnode vrouter $(VROUTER_OUT)		

clean:
	cargo clean --manifest-path vnode/Cargo.toml
	cargo clean --manifest-path vnode/Cargo.toml
	rm ./vnode
	rm ./vrouter