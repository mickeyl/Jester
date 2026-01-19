.PHONY: dev32 dev64 build32 build64 release clean install bridges bridge32 bridge64

# Bridge targets - build both bridges for full DLL support
bridge32:
	cargo build --manifest-path j2534-bridge/Cargo.toml --target i686-pc-windows-msvc

bridge64:
	cargo build --manifest-path j2534-bridge/Cargo.toml --target x86_64-pc-windows-msvc

bridges: bridge32 bridge64

# Development targets - always build both bridges for fault isolation
dev64: bridges
	cargo tauri dev --target x86_64-pc-windows-msvc

dev32: bridges
	cargo tauri dev --target i686-pc-windows-msvc

# Build targets
build32: bridges
	cargo tauri build --target i686-pc-windows-msvc

build64: bridges
	cargo tauri build --target x86_64-pc-windows-msvc

# Release target - builds both 32-bit and 64-bit versions
release: build32 build64
	@echo "Release builds complete!"
	@echo "32-bit: src-tauri/target/i686-pc-windows-msvc/release/"
	@echo "64-bit: src-tauri/target/x86_64-pc-windows-msvc/release/"

# Install npm dependencies
install:
	npm install

# Clean build artifacts
clean:
	cargo clean --manifest-path src-tauri/Cargo.toml
	cargo clean --manifest-path j2534-bridge/Cargo.toml
	rm -rf dist

# Add Rust targets if not already installed
setup-targets:
	rustup target add i686-pc-windows-msvc
	rustup target add x86_64-pc-windows-msvc
