# AGENTS.md

High-signal guidance for OpenCode agents working in the RustSBI repository.
Verify against the executable sources (`.cargo/config.toml`, `xtask/`, `.github/workflows/`) if anything here looks stale.

## Specifications
在specs/md目录下存放了risc-v规范或其他规范文件的md格式，开发过程中如果需要查阅risc-v规范或其他规范请从此处读取。spec/pdf目录中存放规范的原始pdf文件，如果不存在对应的md文件请将pdf转化成md格式并整理成人类可读的格式，然后将md文件存放在specs/md目录下。

## Toolchain

- **Nightly Rust is required** (pinned to `nightly-2026-05-11` in `rust-toolchain.toml`). Edition 2024, resolver 3, and nightly-only rustflags mean stable will not build most of the workspace.
- Components already pinned: `rustfmt`, `clippy`, `llvm-tools-preview`, `rust-src`.
- The `rustsbi` library has an MSRV of 1.88.0, but CI only `cargo check`s (never tests) the library on an older toolchain (`nightly-2025-04-22`). Use the pinned nightly for everything else.
- Install RISC-V targets before building: `rustup target add riscv64gc-unknown-none-elf riscv64imac-unknown-none-elf riscv32imac-unknown-none-elf`.

## Workspace layout

Cargo workspace with three areas plus the task runner:

- `library/` — publishable crates: `rustsbi`, `sbi-spec`, `sbi-rt`, `sbi-testing`, `macros`, `riscv-aia`, `riscv-cove`, `riscv-cove-rt`, `penglai`, `pmpm`, `smm`.
- `prototyper/` — `rustsbi-prototyper` (firmware binary) + `rustsbi-test-kernel` + `rustsbi-bench-kernel`.
- `arceboot/` — `arceboot` bootloader binary (built on ArceOS components).
- `xtask/` — build task runner (not published).

**`default-members` excludes** `sbi-testing`, `riscv-cove-rt`, `smm`, `prototyper/*`, `arceboot`, `xtask`. Plain `cargo build` / `cargo test` at the repo root only touch the default set — always pass `-p <crate>` for anything else.

## Build commands — use the cargo aliases, not raw cargo

Defined in `.cargo/config.toml` as aliases over `xtask`:

| Command | Result |
|---|---|
| `cargo prototyper` | Dynamic-mode firmware → `target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.{elf,bin}` |
| `cargo prototyper --payload <PATH>` | Payload mode → `...-payload.{elf,bin}` |
| `cargo prototyper --jump` | Jump mode → `...-jump.{elf,bin}` |
| `cargo prototyper --features hypervisor` | Adds `+h` target feature (PIE is always on) |
| `cargo test-kernel` | Build `rustsbi-test-kernel` → `target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin` |
| `cargo test-kernel --pack` | Pack into `.itb` (requires `mkimage` from `u-boot-tools`) |
| `cargo bench-kernel` | Build `rustsbi-bench-kernel` (riscv64imac) |
| `cargo arceboot` | Build arceboot (riscv64gc); auto-installs `axconfig-gen` if missing |
| `cargo arceboot --payload` | Also build prototyper with arceboot as payload |
| `cargo arceboot --qemu` | Build then launch QEMU (`--display` for virtio-gpu) |
| `cargo xtask <subcmd>` | Run xtask directly |

The xtask handles: copying `prototyper/prototyper/config/default.toml` → `target/config.toml`, `-Z build-std=core,alloc`, PIE rustflags, `rust-objcopy` ELF→bin, and mode-suffixed output names. **Do not call `cargo build -p rustsbi-prototyper` directly** — you'll miss the config copy, build-std, and objcopy steps, and clippy in CI requires `cargo prototyper` to run first.

### Build target per crate (non-obvious)

- `rustsbi-prototyper`, `arceboot` → **`riscv64gc-unknown-none-elf`**
- `rustsbi-test-kernel`, `rustsbi-bench-kernel` → **`riscv64imac-unknown-none-elf`**
- `rustsbi`, `sbi-rt`, `sbi-spec`, `sbi-testing`, `penglai` → `riscv64imac-unknown-none-elf` (plus `riscv32imac` for most)

> `prototyper/README.md` claims prototyper outputs land in `target/riscv64imac-unknown-none-elf/release/`. That is **stale** — actual output is under `target/riscv64gc-unknown-none-elf/release/` (see `xtask/src/prototyper.rs` and `.github/scripts/prototyper-qemu-boot.sh`).

### Required external tools

- `cargo-binutils` (`rust-objcopy`) — required by every xtask build command.
- `u-boot-tools` (`mkimage`) — only for `cargo test-kernel --pack`.
- `axconfig-gen` — for arceboot (xtask auto-installs it).
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
cargo test -p penglai
cargo test -p rustsbi-prototyper

# Library cross-compile
cargo build -p rustsbi --target riscv64imac-unknown-none-elf
cargo build -p rustsbi --target riscv64imac-unknown-none-elf --features machine,forward

# Clippy (pre-commit enforces -D warnings; CI clippy is non-blocking / continue-on-error)
cargo clippy -p <crate> --target riscv64imac-unknown-none-elf -- -D warnings
```

`forward` is **not** unit-tested in CI because it requires a RISC-V target to compile.

### Prototyper QEMU boot tests

Build the artifacts first, then run the CI script:

```bash
cargo test-kernel
cargo bench-kernel
cargo prototyper
cargo prototyper --jump
cargo prototyper --payload target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin
.github/scripts/prototyper-qemu-boot.sh <payload|dynamic|jump> <test|bench>
```

The script greps the QEMU log for `Hello RustSBI!` and SBI test-pass markers, and retries **only** on timeout (exit 124). Requires `qemu-system-riscv64`.

### ArceBoot integration tests

Heavy: EDK2 build (`arceboot/scripts/test/build_edk2.sh`), disk image (`disk.sh`), ESP (`make_esp.sh`); needs `uuid-dev`, `qemu-system-misc`, Python 3.12. See `.github/workflows/arceboot.yml`. The `openEuler AIA` workflow boots openEuler 25.09 in Docker and only runs on a specific path filter.

### 新增功能后的 prototyper 测试与运行纪律
- **支持自主编译与测试**：在 prototyper 固件或 `rdsm` 库中加入/修改扩展后，允许代理直接执行 `cargo check`、`cargo test`、`cargo prototyper` 编译并使用 NEMU（如 `/home/x402/smmtt/NEMU-cove/build/riscv64-nemu-interpreter -b <payload> -I 10000000`）运行验证。
- **Token 节流约束**：
  - 严禁执行未加过滤的巨型输出命令（如裸跑 `readelf -s`、`objdump`、`rust-nm` 或全量未截断的 build trace）。
  - 符号与反汇编排查必须使用 `grep` / `head -n 30` / `tail -n 30` 管道，单次输出严格控制在 50 行内。

## Conventions & PR checks

- **DCO**: every non-merge commit must include a `Signed-off-by:` line (`.github/workflows/DCO.yml`).
- **Changelog**: changes under `library/{sbi-rt, sbi-spec, sbi-testing, rustsbi, macros}` must update that crate's `CHANGELOG.md` (`.github/workflows/Changelog.yml`).
- **Conventional commits** for prototyper (`feat`, `fix`, `doc`, `perf`, `refactor`, `style`, `revert`, `test`, `chore`); changelog generated via `git-cliff` (`prototyper/cliff.toml`). Commits containing Han characters or `[skip` are skipped by cliff.
- `prototyper/.pre-commit-config.yaml` runs `cargo fmt`, `typos`, `cargo check`, and `cargo clippy -- -D warnings` across all non-xtask packages (targeting `riscv64imac`). Install with `pre-commit install` from the `prototyper/` directory.
- `typos` config `prototyper/_typos.toml` whitelists `rela`, `sie`, `stip` — don't "fix" those.

## Env / config quirks

- `.cargo/config.toml` sets `AX_CONFIG_PATH=arceboot/.axconfig.toml` (relative) for arceboot, and nightly rustflags `-Zcrate-attr=feature(core_io) -Aunused-features` for `riscv64gc-unknown-none-elf`.
- Prototyper reads `PROTOTYPER_FDT_PATH` and `PROTOTYPER_PAYLOAD_PATH` env vars (equivalent to `--fdt` / `--payload`).
- `Cargo.lock` is gitignored (library-first workspace); it exists locally but isn't tracked.
- `specs/` is gitignored (AI tooling scratch).
- `docs/` is an mdBook in Chinese (`docs/book.toml`).
