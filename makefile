# Makefile

TARGET_DIR=/tmp/codecrafters-redis-target
TARGET=$(TARGET_DIR)/release/redis-starter-rust
MANIFEST_PATH=Cargo.toml

.PHONY: all build run

all: build run

build:
	cd $(shell dirname $(realpath $(firstword $(MAKEFILE_LIST)))) && \
	cargo build --release --target-dir=$(TARGET_DIR) --manifest-path=$(MANIFEST_PATH)

run: build
	$(TARGET) $(ARGS)

kill:
	kill -9 $(shell lsof -t -i:6379)