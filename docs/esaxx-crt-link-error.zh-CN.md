# esaxx-rs CRT 链接错误（LNK2038）——复现与归因分析

> 日期：2026-08-06
> 结论状态：**已确认是 xberg 上游（传递依赖链）问题**，已在干净 CI 环境复现；esaxx-rs 已 3 年未维护，xberg 需自行规避。

## 1. 问题现象

对 `xberg_ceshi`（最小复现项目）在 Windows MSVC 下执行：

```bash
cargo build --features xberg-full   # xberg-full = xberg/full-no-heic
```

在**最终链接阶段**报错：

```
libesaxx_rs-*.rlib(esaxx.o): error LNK2038: 检测到"RuntimeLibrary"的不匹配项:
值"MT_StaticRelease"不匹配值"MD_DynamicRelease"（libxberg_tesseract-*.rlib(capi.obj)）
LINK : warning LNK4098: 默认库"MSVCRTD"与其他库的使用冲突；请使用 /NODEFAULTLIB:library
LINK : warning LNK4098: 默认库"LIBCMT"与其他库的使用冲突；请使用 /NODEFAULTLIB:library
fatal error LNK1319: 检测到 1 个不匹配项
```

特点：

- 只影响最终二进制链接，编译期不报错；
- `cargo clean` 后干净构建**必现**；
- 与项目名、本地环境无关——**GitHub Actions 全新 Windows runner**（无任何历史 target 缓存）上原样复现。

## 2. CI 干净环境复现（决定性证据）

仓库内 `.github/workflows/ci.yml`（matrix：default / full）在 `windows-latest` 上构建：

- **default**（xberg 原生默认 `tokio-runtime + simd-utf8`）：通过。此路径**不引入 tokenizers → esaxx**。
- **full**（`xberg/full-no-heic`）：在干净 VM 上从零编译所有依赖后，链接报 LNK2038，与本地**逐字一致**。

结论：**非本地环境问题，非缓存假象**。任何启用 xberg full（或 full-no-heic）的 Windows MSVC 项目都会命中。

## 3. 根因链（逐步溯源）

### 3.1 直接根因：esaxx-rs 的 `.static_crt(true)`

crates.io 的 `esaxx-rs 0.1.10` 的 `build.rs`（`cpp` 特性分支内）：

```rust
cc::Build::new()
    .cpp(true)
    .flag("-std=c++11")
    .static_crt(true)      // ← 强制 MSVC 用 /MT 静态 CRT
    .file("src/esaxx.cpp")
    .include("src")
    .compile("esaxx");
```

产出 `esaxx.lib` 携带 `DEFAULTLIB:LIBCMT`（静态 CRT），与 Rust MSVC 工具链其余部分的动态 CRT（`/MDd`，`MSVCRTD`）在链接期冲突。

### 3.2 关键前提：esaxx 的 C++ 只在 `cpp` 特性开启时编译

esaxx-rs 的构建完全被特性门控：

```toml
# esaxx-rs Cargo.toml
[build-dependencies]
cc = { version = "1.0", optional = true }

[features]
cpp = ["cc"]
default = ["cpp"]
```

```rust
// esaxx-rs build.rs（关键）
#[cfg(feature = "cpp")]
fn main() { /* 编译 esaxx.cpp（含 .static_crt(true)） */ }
#[cfg(not(feature = "cpp"))]
fn main() {}   // ← cpp 关闭时：什么都不编译，纯空操作
```

且 `lib.rs` 里 C++ 绑定（`extern "C" { fn esaxx_int32(...) }` 及 `pub fn suffix()`）同样全部 `#[cfg(feature = "cpp")]` 门控。因此：

- **`esaxx-rs/cpp` 关闭 → 完全不编译/不链接 C++ → 不引入静态 CRT → 问题消失。**
- 开启时 esaxx-rs 同时提供纯 Rust 的 `suffix_rs()`（`esa::esaxx_rs`），但 tokenizers 的 `esaxx_fast` 走的是 C++ 版 `suffix()`。

### 3.3 谁打开了 `esaxx-rs/cpp`？——model2vec-rs → tokenizers 0.21.4

用 `cargo tree -e features --features xberg-full -i esaxx-rs` 定位到完整开关链：

```
esaxx-rs v0.1.10
├── esaxx-rs feature "cc"
│   └── esaxx-rs feature "cpp"
│       └── tokenizers feature "esaxx_fast"          ← 打开 C++
└── tokenizers v0.21.4
    └── model2vec-rs v0.2.1
        └── model2vec-rs feature "fancy-regex"
            └── xberg v1.0.14  (feature "static-embeddings" ← full-no-heic)
```

即：

```
xberg/full-no-heic
└── static-embeddings → model2vec-rs（xberg 依赖其 fancy-regex 特性）
    └── model2vec-rs 内嵌 tokenizers 0.21.4，开启 esaxx_fast
        └── tokenizers 的 esaxx_fast = ["esaxx-rs/cpp"]
            └── esaxx-rs 编译 C++ → .static_crt(true) → LNK2038
```

注意：xberg **自身**的 tokenizers 依赖是 `tokenizers = { version = "0.23", default-features = false, features = ["fancy-regex"] }`（**没有** esaxx_fast），所以 xberg 直接用的 tokenizers 0.23 是干净的。真正把 `esaxx_fast` 带进来的是 **model2vec-rs**（其 `fancy-regex` 特性转发到 tokenizers 的 `esaxx_fast`），且它锁的是 **tokenizers 0.21.4**。

## 4. 修复路径分析

### 4.1 为什么不能靠改 esaxx-rs

`Narsil/esaxx-rs` 最新提交约在 **3 年前**，处于事实上的无人维护状态。向其提交 PR 移除 `.static_crt(true)` 即使被接受，也大概率不会发布新版本、不会进 crates.io。因此**不能把修复寄托于 esaxx-rs 上游**。

### 4.2 可行的修复方向（按代价从低到高）

#### 方向 A：让 `esaxx-rs/cpp` 保持关闭（最干净）

由于 C++ 编译完全由 `esaxx-rs/cpp` 特性门控，只要整个依赖图中**没有**任何 crate 开启 `esaxx_fast` / `esaxx-rs/cpp`，问题就根本不存在。xberg 侧可做：

1. **检查/修正 model2vec-rs 的特性转发**：model2vec-rs 的 `fancy-regex` 特性开启了 `tokenizers/esaxx_fast`。给 model2vec-rs 提 PR（或 fork 后改），让它的 tokenizers 依赖使用 `default-features = false`（不启用 esaxx_fast），只保留 `fancy-regex`。
2. **升级 tokenizers 版本**：model2vec-rs 内嵌的 tokenizers 0.21.4 较旧；若 model2vec-rs 升级到 tokenizers 0.22/0.23 且同样关掉 esaxx_fast，问题消失。

#### 方向 B：xberg 自身加 `[patch.crates-io]`（自建 bin/CLI 可行，库用户无效）

```toml
[patch.crates-io]
esaxx-rs = { git = "https://github.com/<your-org>/esaxx-rs.git" }  # fork 去掉 static_crt
```

- 对 xberg 自己的 workspace（xberg-cli、xberg-ffi 等）**有效**；
- 但对**库的终端用户无效**——`[patch]` 只对声明它的根项目生效，下游 `cargo add xberg` 时不会继承 patch。

#### 方向 C：终端用户自行加 `[patch.crates-io]`（已验证，作为文档兜底）

在用户自己的根 Cargo.toml 加：

```toml
[patch.crates-io]
esaxx-rs = { git = "https://github.com/launcher-rs/esaxx-rs.git" }
```

修复版 fork 已推送到 `github.com/launcher-rs/esaxx-rs`（改动：`build.rs` 两处 `#[cfg(feature="cpp")]` 分支里删掉 `.static_crt(true)`），本地与 CI 均已验证通过。

#### 方向 D：长期根除——让 tokenizers 不再强制 esaxx_fast

最彻底的方案是推动 **huggingface/tokenizers**：要么把 `esaxx-rs` 改为可选依赖（仅 BPE 训练/`esaxx_fast` 需要，纯推理场景不需要），要么把 `esaxx_fast` 从默认特性里去掉。这与 model2vec-rs 方向 A 类似，但从源头解决。

## 5. 结论

- **问题属于 xberg 的传递依赖链**：xberg/full-no-heic → static-embeddings → model2vec-rs → tokenizers 0.21.4 `esaxx_fast` → esaxx-rs 0.1.10 编译带 `.static_crt(true)` 的 C++ → Windows MSVC 链接期 LNK2038。
- **已在干净 CI 复现**，排除本地/缓存因素，判定为上游问题。
- esaxx-rs 已 3 年未维护，**修复不能依赖其上游**；xberg 应在自己可控的依赖边（model2vec-rs 的特性转发 / tokenizers 版本）上规避。
- 用户侧立即生效的兜底：根项目加 `[patch.crates-io] esaxx-rs = { git = ... }`（指向移除 static_crt 的 fork）。
