# Makefile

TARGET_DIR=/tmp/codecrafters-redis-target
TARGET=$(TARGET_DIR)/release/redis-starter-rust
MANIFEST_PATH=Cargo.toml

.PHONY: all build run test

all: build run

build:
cd $(shell dirname $(realpath $(firstword $(MAKEFILE_LIST)))) && \
	cargo build --release --target-dir=$(TARGET_DIR) --manifest-path=$(MANIFEST_PATH)

run: build
$(TARGET) $(ARGS)

kill:
kill -9 $(shell lsof -t -i:6379)

# 추가된 부분: .codecrafters/compile.sh 스크립트 내용

compile:
set -e && \
 cargo build --release --target-dir=$(TARGET_DIR) --manifest-path=$(MANIFEST_PATH)

# 추가된 부분: .codecrafters/run.sh 스크립트 내용

run_codecrafters: compile
set -e && \
 exec $(TARGET) "$@"

# 테스트 단계 추가

test:
cargo test
