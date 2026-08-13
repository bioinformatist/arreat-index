# Arreat Index

Arreat Index 面向中国大陆的《暗黑破坏神 II：狱火重生》（D2R）PC
玩家，目标是提供安全、克制的只读信息辅助。目前仓库只包含可复现的 Rust
工程基线，还没有配置、界面、物品、存储或市场功能。

## 开始开发

进入开发环境并检查工程：

```console
nix develop
cargo test --workspace --all-targets
cargo run -p arreat-app
```

查看版本可运行 `cargo run -q -p arreat-app -- --version`。项目使用稳定版 Rust
1.97.1；完整的本地门禁是 `nix flake check`。

进一步阅读：

- [架构边界](docs/architecture.md)
- [安全边界](docs/safety-boundaries.md)
- [Rust 工具链决策](docs/decisions/0001-rust-toolchain.md)
- [先验证 DD373、再设计市场与界面](docs/decisions/0002-market-validation-before-ui.md)
