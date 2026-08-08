.PHONY: check test fmt lint

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
