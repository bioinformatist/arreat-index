# Arreat Index

Arreat Index 面向中国大陆的《暗黑破坏神 II：狱火重生》（D2R）PC
玩家，目标是提供安全、克制的只读信息辅助。目前仓库只包含可复现的 Rust
工程以及可复用的只读 D2R 数据提取、规范化和审计工具。仓库不发布完整游戏数据。

## 开始开发

进入开发环境并检查工程：

```console
nix develop
cargo test --workspace --all-targets
cargo run -p arreat-app
cargo run -p arreat-data -- --help
```

查看版本可运行 `cargo run -q -p arreat-app -- --version`。项目使用稳定版 Rust
1.97.1；完整的本地门禁是 `nix flake check`。
无需安装 D2R 的夹具流程见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 实验性当前挂单查询

先用现有构建器临时生成名称目录（目录不应提交），再查询一个物品：

```console
research/dd373/build-name-catalog.sh SNAPSHOT research/dd373/name-aliases.json .cache/name-catalog.json
cargo run -q -p arreat-app -- market lookup --catalog .cache/name-catalog.json --item base:r17
```

当前仅支持 `base:r01` 至 `base:r33`，以及目录中能唯一解析的暗金和套装物品。
不支持符文之语、普通底材、孔数、随机词缀、自由词缀匹配或捆绑物品。JSON 只汇总
同一观察时刻的活跃卖家当前挂单；它不是成交价、历史、市场价值、公允价格或购买建议。
模块不输出或保存标题、卖家、联系方式和原始响应。

进一步阅读：

- [架构边界](docs/architecture.md)
- [安全边界](docs/safety-boundaries.md)
- [Rust 工具链决策](docs/decisions/0001-rust-toolchain.md)
- [先验证 DD373、再设计市场与界面](docs/decisions/0002-market-validation-before-ui.md)
