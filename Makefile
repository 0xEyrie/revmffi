.PHONY: all build build-rust build-go test precompile

# Builds the Rust library librevm
BUILDERS_PREFIX := initia/go-ext-builder:0001
# Contains a full Go dev environment in order to run Go tests on the built library
ALPINE_TESTER := initia/go-ext-builder:0001-alpine
CONTRACTS_DIR = ./contracts
USER_ID := $(shell id -u)
USER_GROUP = $(shell id -g)

SHARED_LIB_SRC = "" # File name of the shared library as created by the Rust build system
SHARED_LIB_DST = "" # File name of the shared library that we store
COMPILER_SHARED_LIB_SRC = ""
COMPILER_SHARED_LIB_DST = ""
ifeq ($(OS),Windows_NT)
	SHARED_LIB_SRC = librevmapi.dll
	SHARED_LIB_DST = librevmapi.dll
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


all: test-filenames build test

test-filenames:
	echo $(SHARED_LIB_DST)
	echo $(SHARED_LIB_SRC)
	echo $(COMPILER_SHARED_LIB_DST)
	echo $(COMPILER_SHARED_LIB_SRC)

test: precompile test-rust test-go

test-go:
	RUST_BACKTRACE=full go test -v -count=1 -parallel=1 ./...

test-safety:
	# Use package list mode to include all subdirectores. The -count=1 turns off caching.
	GODEBUG=cgocheck=2 go test -race -v -count=1 -parallel=1 ./...

test-rust: test-compiler test-lib test-e2e test-revm

test-compiler:
	cargo test -p initia-move-compiler

test-revm:
	cargo test -p revm

test-lib:
	cargo test -p initia-move-vm

test-e2e: 
	cargo test -p e2e-move-tests --features testing

build: precompile build-rust build-go

build-rust: build-rust-release

precompile:
	cargo run -p precompile

prebuild-go:
	cargo run -p generate-bcs-go

build-go: prebuild-go
	go build ./...

fmt:
	cargo fmt

update-bindings:
	cp librevm/bindings.h api

# Use debug build for quick testing.
# In order to use "--features backtraces" here we need a Rust nightly toolchain, which we don't have by default
build-rust-debug:
	# cargo build -p revm
	# cargo build -p compiler
	cargo build

	# cp -fp target/debug/$(SHARED_LIB_SRC) api/$(SHARED_LIB_DST)
	cp -fp target/debug/$(SHARED_LIB_SRC) api/$(SHARED_LIB_DST)
	# cp -fp target/debug/librevm.dylib api/librevm.dylib
	make update-bindings

# use release build to actually ship - smaller and much faster
#
# See https://github.com/CosmWasm/wasmvm/issues/222#issuecomment-880616953 for two approaches to
# enable stripping through cargo (if that is desired).
build-rust-release:
	cargo build -p revm --release
	cargo build -p compiler --release
	rm -f api/$(SHARED_LIB_DST)
	rm -f api/$(COMPILER_SHARED_LIB_DST)
	cp -fp target/release/$(SHARED_LIB_SRC) api/$(SHARED_LIB_DST)
	cp -fp target/release/$(COMPILER_SHARED_LIB_SRC) api/$(COMPILER_SHARED_LIB_DST)
	make update-bindings
	@ #this pulls out ELF symbols, 80% size reduction!

clean:
	cargo clean
	@-rm api/bindings.h 
	@-rm api/bindings_compiler.h 
	@-rm librevm/bindings.h
	@-rm libcompiler/bindings_compiler.h
	@-rm api/$(SHARED_LIB_DST)
	@-rm api/$(COMPILER_SHARED_LIB_DST)
	@echo cleaned.

# Creates a release build in a containerized build environment of the static library for Alpine Linux (.a)
release-build-alpine:
	rm -rf target/release
	# build the muslc *.a file
	docker run --rm -u $(USER_ID):$(USER_GROUP)  \
		-v $(shell pwd):/code/ \
		$(BUILDERS_PREFIX)-alpine
	cp artifacts/librevm_muslc.x86_64.a api
	cp artifacts/librevm_muslc.aarch64.a api
	cp artifacts/libcompiler_muslc.x86_64.a api
	cp artifacts/libcompiler_muslc.aarch64.a api
	make update-bindings
	# try running go tests using this lib with muslc
	# docker run --rm -u $(USER_ID):$(USER_GROUP) -v $(shell pwd):/mnt/testrun -w /mnt/testrun $(ALPINE_TESTER) go build -tags muslc ./...
	# Use package list mode to include all subdirectores. The -count=1 turns off caching.
	# docker run --rm -u $(USER_ID):$(USER_GROUP) -v $(shell pwd):/mnt/testrun -w /mnt/testrun $(ALPINE_TESTER) go test -tags muslc -count=1 ./...

# Creates a release build in a containerized build environment of the shared library for glibc Linux (.so)
release-build-linux:
	rm -rf target/release
	docker run --rm -u $(USER_ID):$(USER_GROUP) \
		-v $(shell pwd):/code/ \
		$(BUILDERS_PREFIX)-centos7
	cp artifacts/librevm.x86_64.so api
	cp artifacts/librevm.aarch64.so api
	cp artifacts/libcompiler.x86_64.so api
	cp artifacts/libcompiler.aarch64.so api
	make update-bindings

# Creates a release build in a containerized build environment of the shared library for macOS (.dylib)
release-build-macos:
	rm -rf target/x86_64-apple-darwin/release
	rm -rf target/aarch64-apple-darwin/release
	docker run --rm -u $(USER_ID):$(USER_GROUP) \
		-v $(shell pwd):/code/ \
		$(BUILDERS_PREFIX)-cross build_macos.sh
	cp artifacts/librevm.dylib api
	cp artifacts/libcompiler.dylib api
	make update-bindings

release-build:
	# Write like this because those must not run in parallel
	make release-build-alpine
	make release-build-linux
	make release-build-macos

contracts-gen: $(CONTRACTS_DIR)/*
	@bash ./scripts/contractsgen.sh


flatbuffer-gen:
	@bash ./scripts/flatbuffer-gen.sh
	

