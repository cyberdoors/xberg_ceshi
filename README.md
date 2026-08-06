# xberg_ceshi

最小复现项目：验证 xberg 的 `full` / `full-no-heic` 特性在 Windows MSVC 下的 esaxx-rs CRT 链接错误（LNK2038）。

Minimal reproduction project: verifying xberg's `full` / `full-no-heic` esaxx-rs CRT link error (LNK2038) on Windows MSVC.

## 项目内容 / Contents

| 文件 / File | 说明 / Description |
|---|---|
| `src/main.rs` | 最小触发代码（调用 `xberg::extract`）<br>Minimal trigger (calls `xberg::extract`) |
| `Cargo.toml` | `xberg = "1"`（default）；feature `xberg-full = ["xberg/full-no-heic"]` |
| `.github/workflows/ci.yml` | Windows CI（matrix: default / full），用于在干净 runner 上复现 |
| `docs/esaxx-crt-link-error.zh-CN.md` | 完整排查分析（中文） |
| `docs/esaxx-crt-link-error.en.md` | 完整排查分析（English） |
| `docs/xberg-issue-proposed.md` | 拟提交给 xberg 上游的 issue 草稿 |

## 复现步骤 / Reproduction

```bash
# Windows MSVC
cargo build                       # default：通过（不引入 tokenizers -> esaxx）
cargo build --features xberg-full # full-no-heic：触发 LNK2038
```

## 结论 / Conclusion

- **问题属于 xberg 传递依赖链**：`full-no-heic -> static-embeddings -> model2vec-rs -> tokenizers 0.21.4 (esaxx_fast) -> esaxx-rs 0.1.10` 编译带 `.static_crt(true)` 的 C++，与 Rust MSVC 动态 CRT 在链接期冲突。
- 已在 GitHub Actions 干净 Windows runner 上原样复现，排除本地/缓存因素。
- `Narsil/esaxx-rs` 已约 3 年未维护，修复不能依赖其上游；xberg 应在其可控的依赖边（model2vec-rs 特性转发 / tokenizers 版本）规避。

详见 `docs/`。
