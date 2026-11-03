CARGO   ?= cargo
OUT     := target/dist
NIGHTLY := -Zbuild-std=core,alloc
# ErisMacInjectLib as cargo resolves it: the monorepo's libs/ checkout, or the git dependency.
INJECT   = $(shell $(CARGO) metadata --format-version 1 | perl -ne 'print $$1 and exit if m{"manifest_path":"([^"]*)/crates/inject/Cargo.toml"}')

export MACOSX_DEPLOYMENT_TARGET := 26.0

.PHONY: all payload thread cli app test devhost glasshost self-test clean

all: payload thread cli app

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

# The settings window as an app (the same binary; opened from a bundle it shows the window).
app: cli
	rm -rf $(OUT)/ErisMenus.app
	mkdir -p $(OUT)/ErisMenus.app/Contents/MacOS
	cp resources/Info.plist $(OUT)/ErisMenus.app/Contents/
	cp target/release/erismenus $(OUT)/ErisMenus.app/Contents/MacOS/ErisMenus
	codesign --force --sign - $(OUT)/ErisMenus.app

test:
	$(CARGO) test

# A test app that loads the payload, opens menus, drives them and draws them to PNGs.
devhost: payload
	clang -fobjc-arc -framework AppKit -framework QuartzCore tools/devhost.m -o $(OUT)/devhost

# A test app that puts up every kind of Liquid Glass, captures it as composited and checks it's gone.
glasshost: payload
	clang -fobjc-arc -framework AppKit -framework QuartzCore tools/glasshost.m -o $(OUT)/glasshost

# Maps the payload into the CLI itself and starts it, the way the daemon does (no root needed),
# and checks the thread helper can start a signed thread.
self-test: all
	$(OUT)/erismenus self-test $(OUT)/liberismenus.dylib
	$(OUT)/eris-inject-thread --self-test

clean:
	$(CARGO) clean
