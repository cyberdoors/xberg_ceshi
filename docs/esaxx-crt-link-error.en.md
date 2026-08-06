# esaxx-rs CRT Link Error (LNK2038) — Reproduction & Attribution

> Date: 2026-08-06
> Status: **Confirmed as an upstream (transitive dependency) issue in xberg**; reproduced on a clean CI runner. esaxx-rs has been unmaintained for 3 years, so xberg must work around it itself.

## 1. Symptom

Building `xberg_ceshi` (minimal repro project) on Windows MSVC:

```bash
cargo build --features xberg-full   # xberg-full = xberg/full-no-heic
```

Fails at the **final link stage**:

```
libesaxx_rs-*.rlib(esaxx.o): error LNK2038: mismatch detected for 'RuntimeLibrary':
value 'MT_StaticRelease' doesn't match value 'MD_DynamicRelease' (libxberg_tesseract-*.rlib(capi.obj))
LINK : warning LNK4098: defaultlib 'MSVCRTD' conflicts with use of other libs; use /NODEFAULTLIB:library
LINK : warning LNK4098: defaultlib 'LIBCMT' conflicts with use of other libs; use /NODEFAULTLIB:library
LINK : fatal error LNK1319: 1 mismatches detected
```

Key characteristics:

- Only affects the final binary link; compilation succeeds.
- **Always** reproduces after a clean build.
- Environment-independent: reproduces identically on a **fresh GitHub Actions Windows runner** (no stale `target/` cache).

## 2. Clean CI Reproduction (decisive evidence)

`.github/workflows/ci.yml` (matrix: default / full) builds on `windows-latest`:

- **default** (xberg's native `tokio-runtime + simd-utf8`): passes. This path does **not** pull in tokenizers → esaxx.
- **full** (`xberg/full-no-heic`): compiles every dependency from scratch, then fails at the link step with LNK2038, byte-for-byte identical to the local error.

Conclusion: **not a local environment issue, not a cache artifact.** Any project enabling xberg full (or full-no-heic) on Windows MSVC will hit it.

## 3. Root-Cause Chain

### 3.1 Direct cause: esaxx-rs `.static_crt(true)`

The crates.io `esaxx-rs 0.1.10` `build.rs` (inside the `cpp` feature branch):

```rust
cc::Build::new()
    .cpp(true)
    .flag("-std=c++11")
    .static_crt(true)      // forces MSVC /MT static CRT
    .file("src/esaxx.cpp")
    .include("src")
    .compile("esaxx");
```

The produced `esaxx.lib` carries `DEFAULTLIB:LIBCMT` (static CRT), conflicting at link time with the dynamic CRT (`/MDd`, `MSVCRTD`) used by the rest of the Rust MSVC toolchain.

### 3.2 Key premise: the C++ only compiles when the `cpp` feature is on

esaxx-rs's build is fully feature-gated:

```toml
# esaxx-rs Cargo.toml
[build-dependencies]
cc = { version = "1.0", optional = true }

[features]
cpp = ["cc"]
default = ["cpp"]
```

```rust
// esaxx-rs build.rs (key)
#[cfg(feature = "cpp")]
fn main() { /* compiles esaxx.cpp (incl. .static_crt(true)) */ }
#[cfg(not(feature = "cpp"))]
fn main() {}   // with cpp off: no-op, nothing compiled
```

And in `lib.rs` the C++ binding (`extern "C" { fn esaxx_int32(...) }` and `pub fn suffix()`) is likewise fully `#[cfg(feature = "cpp")]`-gated. Therefore:

- **`esaxx-rs/cpp` off → no C++ is compiled/linked → no static CRT → problem gone.**
- When on, esaxx-rs also offers a pure-Rust `suffix_rs()` (`esa::esaxx_rs`), but tokenizers' `esaxx_fast` uses the C++ `suffix()`.

### 3.3 Who turns on `esaxx-rs/cpp`? — model2vec-rs → tokenizers 0.21.4

`cargo tree -e features --features xberg-full -i esaxx-rs` reveals the switch chain:

```
esaxx-rs v0.1.10
├── esaxx-rs feature "cc"
│   └── esaxx-rs feature "cpp"
│       └── tokenizers feature "esaxx_fast"          <- switches on C++
└── tokenizers v0.21.4
    └── model2vec-rs v0.2.1
        └── model2vec-rs feature "fancy-regex"
            └── xberg v1.0.14  (feature "static-embeddings" <- full-no-heic)
```

i.e.:

```
xberg/full-no-heic
└── static-embeddings -> model2vec-rs (xberg enables its fancy-regex feature)
    └── model2vec-rs bundles tokenizers 0.21.4, enabling esaxx_fast
        └── tokenizers esaxx_fast = ["esaxx-rs/cpp"]
            └── esaxx-rs compiles C++ -> .static_crt(true) -> LNK2038
```

Note: xberg's **own** tokenizers dependency is `tokenizers = { version = "0.23", default-features = false, features = ["fancy-regex"] }` (no `esaxx_fast`), so the tokenizers 0.23 xberg uses directly is clean. The crate that actually brings in `esaxx_fast` is **model2vec-rs** (its `fancy-regex` feature forwards to tokenizers' `esaxx_fast`), and it pins **tokenizers 0.21.4**.

## 4. Fix-Path Analysis

### 4.1 Why the esaxx-rs upstream cannot be relied on

`Narsil/esaxx-rs`'s latest commit is roughly **3 years old** — effectively unmaintained. Even if a PR to remove `.static_crt(true)` were accepted, a new crates.io release is unlikely. **Do not depend on esaxx-rs upstream fixing this.**

### 4.2 Feasible directions (lowest cost first)

#### Option A: Keep `esaxx-rs/cpp` off (cleanest)

Because the C++ build is fully gated by the `esaxx-rs/cpp` feature, the problem disappears entirely as long as **no crate in the graph** enables `esaxx_fast` / `esaxx-rs/cpp`. On the xberg side:

1. **Fix model2vec-rs's feature forwarding**: its `fancy-regex` feature enables `tokenizers/esaxx_fast`. Open a PR to model2vec-rs (or fork it) so its tokenizers dependency uses `default-features = false` (no `esaxx_fast`), keeping only `fancy-regex`.
2. **Bump the tokenizers version**: model2vec-rs bundles an old tokenizers 0.21.4; if it upgrades to 0.22/0.23 with `esaxx_fast` also turned off, the problem disappears.

#### Option B: xberg adds `[patch.crates-io]` (works for xberg's own bins/CLI, not for library consumers)

```toml
[patch.crates-io]
esaxx-rs = { git = "https://github.com/<your-org>/esaxx-rs.git" }  # fork with static_crt removed
```

- Works for xberg's own workspace (xberg-cli, xberg-ffi, etc.);
- Does **not** help library consumers — `[patch]` only applies to the root project that declares it; downstream `cargo add xberg` does not inherit it.

#### Option C: End users add `[patch.crates-io]` themselves (verified; keep as documented fallback)

In the user's own root `Cargo.toml`:

```toml
[patch.crates-io]
esaxx-rs = { git = "https://github.com/launcher-rs/esaxx-rs.git" }
```

The fixed fork is pushed at `github.com/launcher-rs/esaxx-rs` (change: removed `.static_crt(true)` from both `#[cfg(feature = "cpp")]` branches of `build.rs`); verified locally and on CI.

#### Option D: Long-term root fix — stop tokenizers from forcing esaxx_fast

The most thorough fix is to push **huggingface/tokenizers** to either make `esaxx-rs` an optional dependency (it is only needed for BPE training / `esaxx_fast`; pure-inference users don't need it) or drop `esaxx_fast` from the default features. Same idea as Option A, but at the source.

## 5. Conclusion

- **This is an xberg transitive-dependency issue**: xberg/full-no-heic → static-embeddings → model2vec-rs → tokenizers 0.21.4 `esaxx_fast` → esaxx-rs 0.1.10 compiles C++ with `.static_crt(true)` → Windows MSVC link-time LNK2038.
- **Reproduced on a clean CI runner**; local/cache factors ruled out; judged an upstream issue.
- esaxx-rs has been unmaintained for 3 years, so **the fix cannot rely on its upstream**; xberg should work around it on a dependency edge it controls (model2vec-rs feature forwarding / tokenizers version).
- Immediate user-side fallback: add `[patch.crates-io] esaxx-rs = { git = ... }` (pointing at the fork with `static_crt` removed) in the root project.
