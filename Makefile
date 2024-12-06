ROOT_DIR := $(shell pwd)

LIBRARY_DIR := ./tcp-imp/library
VNODE_DIR := ./tcp-imp/vnode

VHOST_OUT := $(ROOT_DIR)/vhost
VROUTER_OUT := $(ROOT_DIR)/vrouter

debug: build_debug

build_debug:
	cargo build --manifest-path $(ROOT_DIR)/tcp-imp/Cargo.toml
	cp $(ROOT_DIR)/tcp-imp/target/debug/vnode $(VHOST_OUT)
	cp $(ROOT_DIR)/tcp-imp/target/debug/vnode $(VROUTER_OUT)

build:
	cargo build --manifest-path $(ROOT_DIR)/tcp-imp/Cargo.toml --release
	cp $(ROOT_DIR)/tcp-imp/target/release/vnode $(VHOST_OUT)
	cp $(ROOT_DIR)/tcp-imp/target/release/vnode $(VROUTER_OUT)		

clean:
	cargo clean --manifest-path $(VNODE_DIR)/Cargo.toml
	rm vhost
	rm vrouter
