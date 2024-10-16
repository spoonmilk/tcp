ROOT_DIR := $(shell pwd)

LIBRARY_DIR := ./ip-imp/library
VNODE_DIR := ./ip-imp/vnode

VHOST_OUT := $(ROOT_DIR)/vhost
VROUTER_OUT := $(ROOT_DIR)/vrouter

debug: build_debug

build_debug:
	cargo build --manifest-path $(ROOT_DIR)/ip-imp/Cargo.toml
	cp $(ROOT_DIR)/ip-imp/target/debug/vnode $(VHOST_OUT)
	cp $(ROOT_DIR)/ip-imp/target/debug/vnode $(VROUTER_OUT)

build:
	cargo build --manifest-path $(ROOT_DIR)/ip-imp/Cargo.toml --release
	cp $(ROOT_DIR)/ip-imp/target/release/vnode $(VHOST_OUT)
	cp $(ROOT_DIR)/ip-imp/target/release/vnode $(VROUTER_OUT)		

clean:
	cargo clean --manifest-path $(VNODE_DIR)/Cargo.toml
	rm $(ROOT_DIR)/vnode
	rm $(ROOT_DIR)/vrouter