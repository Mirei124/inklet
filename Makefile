.PHONY: check test fmt lint build-release app-bundle

# 一键质量检查（格式化 + 静态检查 + 测试）
check: fmt lint typecheck test

# 格式化（写入）
fmt:
	pnpm format
	cd src-tauri && cargo fmt

# 静态检查（不写入）
lint:
	pnpm format:check
	pnpm lint
	cd src-tauri && cargo fmt --check && cargo clippy -- -D warnings

# 类型检查
typecheck:
	pnpm typecheck

# 测试
test:
	pnpm test
	cd src-tauri && cargo test

# 一键打包：构建前端 + release 二进制（自包含，前端已嵌入）
build-release:
	pnpm build
	cd src-tauri && cargo build --release
	@echo "产物: src-tauri/target/release/inklet"

# macOS：在 build-release 基础上打包成可分发的 Inklet.app
# （含 Info.plist，LSUIElement 使应用不出现在 Dock，仅作为桌面覆盖层）
app-bundle: build-release
	@echo "打包 Inklet.app ..."
	rm -rf dist/Inklet.app
	mkdir -p dist/Inklet.app/Contents/MacOS dist/Inklet.app/Contents/Resources
	cp src-tauri/target/release/inklet dist/Inklet.app/Contents/MacOS/inklet
	cp src-tauri/Info.plist dist/Inklet.app/Contents/Info.plist
	cp src-tauri/icons/icon.icns dist/Inklet.app/Contents/Resources/icon.icns
	@echo "产物: dist/Inklet.app"
