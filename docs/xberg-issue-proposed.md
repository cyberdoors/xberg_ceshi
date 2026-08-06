# Proposed Issue: xberg-io/xberg — Windows MSVC `full` / `full-no-heic` build fails with LNK2038

> 用途：直接提交到 https://github.com/xberg-io/xberg/issues/new/choose 的 Bug Report 模板。
> 说明：xberg 的 Bug Report 模板字段为 Description / Steps to reproduce / Relevant files and configuration。

## Title

`bug: Windows MSVC full/full-no-heic build fails with LNK2038 (esaxx-rs static CRT vs /MD)`

## Description

Using xberg with the `full` (or `full-no-heic`) feature on **Windows / MSVC** fails at the final link step with `LNK2038`:

```
error: linking with link.exe failed
libesaxx_rs-*.rlib(...esaxx.o): error LNK2038: RuntimeLibrary mismatch detected:
value 'MT_StaticRelease' doesn't match value 'MD_DynamicRelease' in libxberg_tesseract-*.rlib(capi.obj)
LINK : fatal error LNK1319: 1 mismatches detected
```

Default features (`tokio-runtime + simd-utf8`) pass. The failure appears as soon as the `tokenizers` -> `esaxx-rs` chain is enabled, and reproduces on a fresh GitHub Actions `windows-latest` runner (clean `target/`), so it is not environment- or cache-related.

## Steps to reproduce

```bash
cargo new repro && cd repro
cargo add xberg --features full-no-heic
cargo build        # x86_64-pc-windows-msvc
```

(Using `--features full` also hits it; see the dependency chain below.)

## Relevant files and configuration

Dependency chain enabling the offending C++ (from `cargo tree -e features -i esaxx-rs`):

```
xberg/full -> static-embeddings -> model2vec-rs (feature "fancy-regex")
  -> tokenizers 0.21.4 (feature "esaxx_fast")
    -> esaxx-rs 0.1.10 (feature "cpp")
      -> build.rs .static_crt(true)   # forces /MT static CRT, conflicts with Rust /MD
```

Key facts:

- esaxx-rs's C++ is fully gated behind its `cpp` feature (`build.rs` and the `extern`/`suffix()` bindings are all `#[cfg(feature = "cpp")]`). With `cpp` off, esaxx-rs is a no-op and the CRT mismatch disappears entirely.
- The actual trigger is **model2vec-rs**'s `fancy-regex` feature enabling `tokenizers/esaxx_fast` (and it pins old **tokenizers 0.21.4**).
- xberg's own `tokenizers` dependency (`0.23`, `default-features = false`, `features = ["fancy-regex"]`) does **not** enable `esaxx_fast`; it is clean.
- `Narsil/esaxx-rs` is effectively unmaintained (~3 years), so an upstream esaxx-rs fix is not a realistic path.

Suggested fix (xberg side):

1. Make model2vec-rs use `tokenizers` with `default-features = false` (keep `fancy-regex`, drop `esaxx_fast`), either via upstream PR or a fork.
2. Bump model2vec-rs's tokenizers from 0.21.4 to 0.22/0.23 with `esaxx_fast` off.
3. Long term: ask `huggingface/tokenizers` to make `esaxx-rs` optional (it is only needed for BPE training / `esaxx_fast`; pure-inference users do not need it) or drop `esaxx_fast` from default features.

Repro project + full bilingual analysis (zh-CN / en): `https://github.com/cyberdoors/xberg_ceshi` (see `docs/`).
