.PHONY: dev32 dev64 build32 build64 release clean install

# Development targets
dev32:
	cargo tauri dev --target i686-pc-windows-msvc

dev64:
	cargo tauri dev --target x86_64-pc-windows-msvc

# Build targets
build32:
	cargo tauri build --target i686-pc-windows-msvc

build64:
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
	rm -rf dist

# Add Rust targets if not already installed
setup-targets:
	rustup target add i686-pc-windows-msvc
	rustup target add x86_64-pc-windows-msvc
