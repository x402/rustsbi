# AGENTS.md

High-signal guidance for AI coding agents (ZCode; OpenCode/Codex delegation was retired on 2026-09-07 — see the top-level `/home/x402/CoVE/AGENTS.md` §3.0) working in the RustSBI repository.
Verify against the executable sources (`.cargo/config.toml`, `xtask/`, `.github/workflows/`) if anything here looks stale.

## Specifications
在specs/md目录下存放了risc-v规范或其他规范文件的md格式，开发过程中如果需要查阅risc-v规范或其他规范请从此处读取。spec/pdf目录中存放规范的原始pdf文件，如果不存在对应的md文件请将pdf转化成md格式并整理成人类可读的格式，然后将md文件存放在specs/md目录下。

## Toolchain

- **Nightly Rust is required** (pinned in `rust-toolchain.toml`). Edition 2024, resolver 3, and nightly-only rustflags mean stable will not build most of the workspace.
- Components already pinned: `rustfmt`, `clippy`, `llvm-tools-preview`, `rust-src`.
- Install RISC-V targets before building: `rustup target add riscv64gc-unknown-none-elf riscv64imac-unknown-none-elf riscv32imac-unknown-none-elf`.

## Workspace layout

Cargo workspace, synced with upstream `rustsbi/rustsbi` main (2026-09-11):

- `library/` — publishable crates: `rustsbi`, `sbi-spec`, `sbi-rt`, `sbi-testing`, `macros`, `riscv-aia`, `riscv-cove`, `riscv-cove-rt`, `rpmi`, `penglai`, `pmpm`, `smm`.
- `firmware/` — `rustsbi-prototyper` (firmware binary), `rustsbi-test-kernel`, `rustsbi-bench-kernel`, plus the `macros` and `runtime` support crates.
- `xtask/` — build task runner (not published).

**CoVE divergence (keep minimal)**: all RDSM code lives in the sibling `../cove-sw`
repository (`crates/rdsm-abi`, `crates/rdsm`, `crates/rdsm-fw`). This repo only keeps:

- `firmware/prototyper/src/sbi/rdsm.rs` — feature-gated shim (`#[cfg(feature = "rdsm")]`
  re-exports `rdsm-fw`; no-op stubs otherwise). All integration points are gated so a
  feature-off build stays byte-for-byte upstream behaviour.
- Optional path dependency `rdsm-fw = { path = "../../../cove-sw/crates/rdsm-fw", optional = true }`.
- xtask payload mode auto-enables the `rdsm` feature.

**`default-members`** covers the host-testable libraries. Plain `cargo build` /
`cargo test` at the repo root only touch the default set — always pass `-p <crate>` for anything else.

## Build commands — use the cargo aliases, not raw cargo

`.cargo/config.toml` aliases: `cargo prototyper` → `xtask prototyper`, `cargo xtask` → run xtask directly. Upstream xtask now uses a subcommand CLI:

| Command | Result |
|---|---|
| `cargo prototyper build` | Dynamic-mode firmware → `target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.{elf,bin}` |
| `cargo prototyper build jump` | Jump mode → `...-jump.{elf,bin}` |
| `cargo prototyper build --fdt <DTB> payload <PATH>` | Payload mode (auto-adds the `rdsm` feature) → `...-payload.{elf,bin}` |
| `cargo prototyper build test` / `bench` | Kernel build with payload-mode firmware, then QEMU boot |

**Do not call `cargo build -p rustsbi-prototyper` directly** — you'll miss the generated
includes, build-std, and objcopy steps. `--fdt` is a `build`-level option and must precede
the mode subcommand.

### Required external tools

- `cargo-binutils` (`rust-objcopy`) — required by every xtask build command.
- `qemu-system-misc` (`qemu-system-riscv64`) — for boot tests.

## rustsbi library features

`library/rustsbi/Cargo.toml`, `default = []`:

- `machine` — for real M-mode RISC-V (pulls in the `riscv` crate). Use for firmware.
- `forward` — for hypervisors running on top of another SBI (pulls in `sbi-rt`). **Only compiles on RISC-V targets**, not on the native host.
- No features — for cross-architecture VM/emulator development.

## Verification

```bash
# Format (nightly rustfmt required for 2024 edition)
cargo fmt --check

# Library unit tests (native host)
cargo test -p rustsbi                       # no features
cargo test -p rustsbi --features machine
cargo test -p sbi-spec
cargo test -p sbi-rt

# Firmware cross-compile checks
cargo check -p rustsbi-prototyper --target riscv64gc-unknown-none-elf                    # upstream mode
cargo check -p rustsbi-prototyper --target riscv64gc-unknown-none-elf --features rdsm    # CoVE mode
```

### CoVE end-to-end regression (the real gate)

```bash
cd /home/x402/CoVE/cove-sw && cargo xtask pack
cd /home/x402/CoVE/rustsbi && cargo prototyper build --fdt /home/x402/CoVE/NEMU/build/nemu.dtb \
    payload /home/x402/CoVE/cove-sw/target/riscv64gc-unknown-none-elf/release/cove-payload.bin
/home/x402/CoVE/NEMU/build/riscv64-nemu-interpreter \
    -b target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.bin -I 20000000
# Expect: [MARKER 16] HOST: ALL COVE E2E TESTS PASSED!
```

- **支持自主编译与测试**：修改固件或 cove-sw 后，允许代理直接执行上述命令验证；观察 PASS 标志（`HOST_STARTED`、`TSM_READY`、`ALL COVE E2E TESTS PASSED` 等）。
- **Token 节流约束**：
  - 严禁执行未加过滤的巨型输出命令（如裸跑 `readelf -s`、`objdump`、`rust-nm` 或全量未截断的 build trace）。
  - 符号与反汇编排查必须使用 `grep` / `head -n 30` / `tail -n 30` 管道，单次输出严格控制在 50 行内。

## Conventions & PR checks

- **DCO**: every non-merge commit must include a `Signed-off-by:` line (`.github/workflows/DCO.yml`).
- **Changelog**: changes under `library/{sbi-rt, sbi-spec, sbi-testing, rustsbi, macros}` must update that crate's `CHANGELOG.md` (`.github/workflows/Changelog.yml`).
- **Conventional commits**; changelog generated via `git-cliff` (`firmware/cliff.toml`). Commits containing Han characters or `[skip` are skipped by cliff.
- `firmware/.pre-commit-config.yaml` runs fmt / typos / check / clippy. `firmware/_typos.toml` whitelists `rela`, `sie`, `stip` — don't "fix" those.

## Env / config quirks

- Nightly rustflags `-Zcrate-attr=feature(core_io) -Aunused-features` apply for `riscv64gc-unknown-none-elf`.
- `Cargo.lock` is tracked again after the upstream sync (2026-09-11).
- `specs/` is gitignored (AI tooling scratch).
- `docs/` is an mdBook in Chinese (`docs/book.toml`).
