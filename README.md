# Arreat Index

Arreat Index 面向中国大陆的《暗黑破坏神 II：狱火重生》（D2R）PC
玩家，目标是提供安全、克制的只读信息辅助。目前仓库包含可复现的 Rust
工程、可复用的只读 D2R 数据提取、规范化和审计工具，以及一个实验性的
D2RLoader 客户端物品身份证明 DLL。仓库不发布完整游戏数据。

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

## Linux 本地目录与实验性当前挂单查询

Linux 上先从本机 D2R 安装只读构建或复用名称目录，再把输出路径显式交给查询命令：

```console
catalog_path=$(arreat-data catalog --game-root /absolute/path/to/d2r)
arreat-app market lookup --catalog "$catalog_path" --item base:r17
```

可用 `--cache-root /absolute/path` 指定缓存根目录；否则使用
`$XDG_CACHE_HOME/arreat-index`，或回退到 `$HOME/.cache/arreat-index`。
首次未命中时会调用只读 CascLib 提取与 OpenCC 1.3.0 转换；输入未变时再次运行会完整
验证并复用缓存，不打开 CASC，也不调用 OpenCC。成功或失败都只保留最终目录文件，不保留
阶段归档或完整快照。Nix 包 `arreat-index-cli` 一次性安装 `arreat-data` 与 `arreat-app`，
其中 `arreat-data` 运行时包含 OpenCC 1.3.0，`arreat-app` 运行时通过
`SSL_CERT_FILE` 绑定 NSS CA Bundle 以发起 DD373 HTTPS 查询。
当前仅支持 Linux 本地安装运行；Windows CI 只保证编译和纯夹具行为。Windows 安装发现、
实际运行验收与图形界面均推迟。

[D2RLoader tooltip identity proof](plugins/d2rloader/README.md) 只在最终物品提示回调中
显示复制出的符文编号、持有数量或暗金/套装表行号。它是面向中国大陆客户端与 Proton
实机验收的实验性身份观察证明，不是完整界面；Windows 运行时、图形界面以及市场价格卡
集成都仍然推迟。Windows 工作流目前只负责构建、测试和暂存 DLL、SDK 许可证及校验值，
不能证明 Proton 实机观察已经成功。

当前仅支持 `base:r01` 至 `base:r33`，以及目录中能唯一解析的暗金和套装物品。
`固定专名`标识的是一件唯一的 `暗金` 或 `套装` 物品，但不固定具体属性值。
`base:r01` 至 `base:r33` 是符文内部 ID 约定，不意味着该范围即代表支持 `底材`。
不支持符文之语、普通底材、孔数、随机词缀、自由词缀匹配或捆绑物品。JSON 只汇总同一观察时刻的活跃卖家当前挂单；它不是成交价、历史、市场价值、公允价格或购买建议。
符文采用 schema 3 的 `Per-item` 模式，返回完整匿名 lot tuple（`quantity_per_lot`、
`lot_price`、`available_lots`、`unit_price`）。最低单价与最低入场价分别保留所有不同的
并列最低 tuple，绝不跨挂单组合字段；
暗金和套装使用 `Per-listing` 模式返回挂牌价格统计。模块只查询、聚合与分析当前挂单，不会执行或自动化
任何交易行为。
模块不输出或保存标题、卖家、联系方式和原始响应。
目录构建只读取游戏安装并写入 Arreat Index 自有缓存，不修改 D2R、Battle.net 或任何
DD373 账户状态，也不会发布暴雪派生的归档、完整快照或完整目录。

进一步阅读：

- [架构边界](docs/architecture.md)
- [安全边界](docs/safety-boundaries.md)
- [Rust 工具链决策](docs/decisions/0001-rust-toolchain.md)
- [先验证 DD373、再设计市场与界面](docs/decisions/0002-market-validation-before-ui.md)
