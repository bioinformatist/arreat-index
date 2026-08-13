# ADR 0001：固定 Rust 工具链

- 状态：已接受
- 日期：2026-08-13

## 决策

项目使用稳定版 Rust 1.97.1、edition 2024 和 Cargo resolver 3。Cargo、Fenix
与 CI 都固定到这一版本，使 Linux 开发环境和 Windows 编译门禁保持一致。

不得仅为尝鲜改用 nightly。只有后续出现稳定版无法满足的具体需求、获得明确批准，
并用日期固定 nightly 工具链且另写 ADR 记录兼容与退出方案时，才允许例外。
