.PHONY: all build build-rust build-go test precompile clean-store

AOT_STORE_PATH := $(HOME)/.aotstore
# Builds the Rust library librevm
BUILDERS_PREFIX := 0xeyrie/librevm-builder:0001
BENCHMARK_PREFIX := 0xeyrie/benchmark:0001
USER_ID := $(shell id -u)
USER_GROUP = $(shell id -g)

SHARED_LIB_SRC = "" # File name of the shared library as created by the Rust build system
SHARED_LIB_DST = "" # File name of the shared library that we store
ifeq ($(OS),Windows_NT)
	# not supported
else
	UNAME_S := $(shell uname -s)
	ifeq ($(UNAME_S),Linux)
		SHARED_LIB_SRC = librevmapi.so
		SHARED_LIB_DST = librevmapi.$(shell rustc --print cfg | grep target_arch | cut  -d '"' -f 2).so
	endif
	ifeq ($(UNAME_S),Darwin)
		SHARED_LIB_SRC = librevmapi.dylib
		SHARED_LIB_DST = librevmapi.dylib
	endif
endif

# lint (macos)
lint:
	@export LLVM_SYS_180_PREFIX=$(shell brew --prefix llvm@18);\
	cargo fix --allow-dirty --allow-staged
	@export LLVM_SYS_180_PREFIX=$(shell brew --prefix llvm@18);\
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	make fmt
	
fmt:
	rustup run nightly cargo fmt
	go fmt

update-bindings:
	cp librevm/bindings.h core/vm

test:
	make build-rust-debug
	go clean -testcache
	go test -v -run TestEofFibWithAOT

clean-store:
	@echo "clean the db: $(AOT_STORE_PATH)"
	@if [ -d "$(AOT_STORE_PATH)" ]; then \
		rm -rf "$(AOT_STORE_PATH)"; \
		echo "Directory $(AOT_STORE_PATH) removed successfully."; \
	else \
		echo "Directory $(AOT_STORE_PATH) does not exist."; \
	fi

# Use debug build for quick testing.
# In order to use "--features backtraces" here we need a Rust nightly toolchain, which we don't have by default
# build in macos to debug
build-rust-debug:
	@export LLVM_SYS_180_PREFIX=$(shell brew --prefix llvm@18);\
	export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH";\
	export LD_LIBRARY_PATH="/opt/homebrew/lib:$LD_LIBRARY_PATH";\
	export RUST_BACKTRACE=full; \
	cargo build
	@cp -fp target/debug/$(SHARED_LIB_SRC) core/vm/$(SHARED_LIB_DST)
	@make update-bindings

build-rust-release:
	@export LLVM_SYS_180_PREFIX=$(shell brew --prefix llvm@18);\
	export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH";\
	export LD_LIBRARY_PATH="/opt/homebrew/lib:$LD_LIBRARY_PATH";\
	cargo build --release
	rm -f core/vm/$(SHARED_LIB_DST)
	cp -fp target/release/$(SHARED_LIB_SRC) core/vm/$(SHARED_LIB_DST)
	make update-bindings

clean:
	cargo clean
	@-rm core/vm/bindings.h
	@-rm librevm/bindings.h
	@-rm core/vm/$(SHARED_LIB_DST)
	@echo cleaned.

# Creates a release build in a containerized build environment of the shared library for glibc Linux (.so)
release-build-linux:
	docker run --rm -v $(shell pwd):/code/ $(BUILDERS_PREFIX)-debian build_gnu_x86_64.sh
	docker run --rm -v $(shell pwd):/code/ $(BUILDERS_PREFIX)-debian build_gnu_aarch64.sh
	cp artifacts/librevmapi.x86_64.so core/vm
	cp artifacts/librevmapi.aarch64.so core/vm
	make update-bindings

# Creates a release build in a containerized build environment of the shared library for macOS (.dylib)
release-build-macos:
	rm -rf target/x86_64-apple-darwin/release
	rm -rf target/aarch64-apple-darwin/release
	docker run --rm -u $(USER_ID):$(USER_GROUP) \
		-v $(shell pwd):/code/ \
		$(BUILDERS_PREFIX)-cross build_macos.sh
	cp artifacts/librevmapi.dylib core/vm
	make update-bindings

release-build:
	# Write like this because those must not run in parallel
	make release-build-linux
	make release-build-macos

protobuf-gen:
	@bash ./scripts/protobufgen.sh