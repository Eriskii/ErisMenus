CARGO   ?= cargo
OUT     := target/dist
NIGHTLY := -Zbuild-std=core,alloc
INJECT  := ../../libs/ErisMacInjectLib

export MACOSX_DEPLOYMENT_TARGET := 26.0

.PHONY: all payload thread cli test devhost self-test clean

all: payload thread cli

# One arm64 build, mapped into every app by the daemon (arm64e apps included).
payload:
	$(CARGO) build --release -p payload $(NIGHTLY) --target aarch64-apple-darwin
	mkdir -p $(OUT)
	cp target/aarch64-apple-darwin/release/liberismenus.dylib $(OUT)/

# eris-inject's thread helper. Must be arm64e: it signs the payload thread's start for arm64e apps.
thread:
	$(CARGO) build --release -p eris-inject-thread $(NIGHTLY) --target arm64e-apple-darwin \
		--manifest-path $(INJECT)/Cargo.toml --target-dir target
	mkdir -p $(OUT)
	cp target/arm64e-apple-darwin/release/eris-inject-thread $(OUT)/

cli:
	$(CARGO) build --release -p erismenus
	mkdir -p $(OUT)
	cp target/release/erismenus $(OUT)/

test:
	$(CARGO) test

# A test app that loads the payload, opens menus, drives them and draws them to PNGs.
devhost: payload
	clang -fobjc-arc -framework AppKit -framework QuartzCore tools/devhost.m -o $(OUT)/devhost

# Maps the payload into the CLI itself and starts it, the way the daemon does (no root needed),
# and checks the thread helper can start a signed thread.
self-test: all
	$(OUT)/erismenus self-test $(OUT)/liberismenus.dylib
	$(OUT)/eris-inject-thread --self-test

clean:
	$(CARGO) clean
