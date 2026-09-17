# Deepwork: add-rdsm-substrate

## Goal
Implement RDSM (Root Domain Security Manager) substrate for RustSBI — new `library/rdsm` crate + prototyper integration. 58 tasks across 12 sections.

## Key Spec Details (from specs/md/smmtt-spec.md v0.49)

### mmpt CSR (0x382) — MXLEN-wide R/W
- MODE (WARL): 0=Bare, 1=Smmpt43, 2=Smmpt52, 3=Smmpt64 (RV64)
- SDID (WARL): SDIDLEN bits, max SDIDMAX=6 (64 domains)
- PPN (WARL): root page of MPT
- Assumed bit layout (SATP-like): MODE[63:60], SDID[59:54], PPN[53:0]
- SDID width discovered via WARL probe (write all-1s, read back, count trailing zeros)
- Smmpt64 root must be 32 KiB aligned

### msdcfg CSR (0x74E) — MXLEN-wide R/W
- SIDN: bits [5:0] (6 bits)
- SEDA: bit 6, SETA: bit 7
- bits [15:8] WPRI, bits [21:16] WPRI
- SSRM: bit 22, SSMM: bit 23, SRL: [27:24], SML: [31:28]

### msideip (0xF4F) — 64-bit RO, bit i = SID i pending
### msideie (0x74F) — 64-bit R/W, bit i = SID i enabled
### MSDEI = mip bit 14; mip.MSDEIP = (msideip & msideie) != 0

### MFENCE.PA / MINVAL.PA — TEMPORARY ENCODING
- MFENCE.PA: func7=0001110(0x0E), func3=000, opcode=SYSTEM(1110011), rd=0
- MINVAL.PA: func7=0001111(0x0F), func3=000, opcode=SYSTEM, rd=0
- MUST mark with TODO: temporary encoding convention

### MPT Structure (Smmpt43 — NEMU target)
- 3 levels: pn[2](9) | pn[1](9) | pn[0](9) | range_offset(16) = 43 bits
- Tables: 512 entries × 8 bytes = 4 KiB, page-aligned
- Leaf covers 16 pages (NUMPGINRANGE=4, 2^4=16), range_offset=16 bits → 2^16/4096=16 pages
- LEVELS=3, MPTESIZE=8

### MPTE Bit Layout (64-bit, assumed)
Non-leaf: PPN[63:10] | Reserved[9:2] | L[1]=0 | V[0]=1
Leaf (non-NAPOT): XWR[15:0][63:16] (16×3bit=48bit) | Reserved[15:3] | N[2]=0 | L[1]=1 | V[0]=1
- XWR[i] at bits [18+i*3 : 16+i*3]
- Page index within leaf: (pa >> 12) & 0xF

### XWR Encoding
- 000=None, 001=R, 010=Reserved, 011=RW, 100=X, 101=RX, 110=Reserved, 111=RWX

### MPT Modes Summary
| Mode | Levels | Root entries | Root size | PA bits |
|------|--------|-------------|-----------|---------|
| Smmpt43 | 3 | 512 | 4 KiB | 43 |
| Smmpt52 | 4 | 512 | 4 KiB | 52 |
| Smmpt64 | 5 | 4096 | 32 KiB | 64 |

## File References
- Root workspace: `Cargo.toml` — members list, default-members exclusion
- Pattern crates: `library/pmpm/Cargo.toml`, `library/smm/Cargo.toml`
- Prototyper features: `prototyper/prototyper/src/sbi/features.rs` (Extension enum, COUNT, iter, as_str)
- Prototyper CSR: `prototyper/prototyper/src/riscv/csr.rs` (CSR constants)
- Prototyper trap: `prototyper/prototyper/src/sbi/trap/mod.rs` (fast_handler, handle_exception, handle_interrupt)
- Prototyper trap handler: `prototyper/prototyper/src/sbi/trap/handler.rs`
- Prototyper early_trap: `prototyper/prototyper/src/sbi/early_trap.rs` (csr_read_allow, csr_write_allow, TrapInfo)
- Prototyper main: `prototyper/prototyper/src/main.rs` (rust_main boot flow)
- Prototyper firmware: `prototyper/prototyper/src/firmware/mod.rs` (set_pmp, boot flow)
- Prototyper platform: `prototyper/prototyper/src/platform/mod.rs` (Platform, BoardInfo)
- Prototyper devicetree: `prototyper/prototyper/src/devicetree.rs` (Cpu, Memory, Tree)
- Prototyper sbi mod: `prototyper/prototyper/src/sbi/mod.rs` (module declarations)
- Prototyper Cargo: `prototyper/prototyper/Cargo.toml`
- Prototyper macros: `prototyper/prototyper/src/macros.rs` (has_csr! macro)
- Prototyper fail: `prototyper/prototyper/src/fail.rs` (fail::stop, unsupported_trap)
- Prototyper heap: `prototyper/prototyper/src/sbi/heap.rs`

## Implementation Plan

### Phase 1: Crate scaffolding (tasks 1.1-1.4) — DIRECT
- Create library/rdsm/Cargo.toml, src/lib.rs, CHANGELOG.md
- Add to workspace members, exclude from default-members
- Verify cargo check -p rdsm

### Phase 2: Library modules (tasks 2-8) — FIXER LANES
2a (sequential): csr.rs + fence.rs (foundational)
2b (parallel after 2a):
  - Fixer A: mpt.rs + domain.rs
  - Fixer B: interrupt.rs + fault.rs + probe.rs

### Phase 3: Prototyper extension probing (tasks 9.1-9.5) — FIXER
- features.rs: add Smsdid/Smmpt/Smsdia variants
- csr.rs: add CSR_MMPT/CSR_MSDCFG constants
- Cargo.toml: add rdsm dependency
- Verify cargo prototyper

### Phase 4: Prototyper RDSM init (tasks 10.1-10.7) — FIXER
- sbi/rdsm.rs: TrapSafeCsr impl, bump allocator, rdsm_init()
- main.rs: call rdsm_init() after hart_features_detection()
- firmware/mod.rs: PMP-before-MPT timing
- Verify cargo prototyper

### Phase 5: Prototyper trap routing (tasks 11.1-11.4) — FIXER
- trap/handler.rs: MPT fault + MSDEI interrupt handling
- Verify cargo prototyper

### Phase 6: NEMU testing (tasks 12.1-12.5) — BLOCKED (requires NEMU hardware)

## Oracle Review (ora-1) — Reconciled

**Verdict**: Plan is executable. Architecture, spec alignment, parallelization all sound.

### Action Items
1. 🔴 Fix NEMU init bug in task 9.1: `features.rs:187-196` has field name typo (`extension` → `extensions`) and missing `mhpm_mask`/`mhpm_bits` fields. Fix when adding new extension variants.
2. ⚠️ Document non-boot-hart gap: Non-boot harts have `mmpt.MODE=Bare` at reset. For Phase 1 (permissive host MPT), this is functionally equivalent. Add note to task 10.5: "Non-boot harts initialized with mmpt.MODE=Bare; must be programmed before MPT restrictions enforced in future phases."
3. ⚠️ Test strategy: Host-testable = MptPerm, SdidAllocator, SidnAllocator, MptFault::from_trap, probe_* with mock TrapSafeCsr. Hardware-required = CSR read/write round-trip, mfence_pa/minval_pa, MptTree with real pages.
4. Watch: Recursive tree destruction — track visited pages or depth limit to avoid double-free on corrupted tables.
5. Watch: Check if `riscv` crate v0.16.0 has MSDEI (mip bit 14) variant. If not, use raw mcause check before `try_into()` in trap handler.
6. MptPageAlloc: Document that `destroy()` is no-op for bump allocators in Phase 1 (can't reclaim individual pages).
7. msdcfg SIDN: Document WLRL semantics — writing invalid SIDN has hardware-visible side effects on S-mode interrupt delivery.

## Phase Status
- Phase 1: COMPLETE ✓ (4/4 tasks)
- Phase 2a: COMPLETE ✓ (csr.rs + fence.rs, 10 tests)
- Phase 2b: COMPLETE ✓ (mpt.rs + domain.rs + interrupt.rs + fault.rs + probe.rs, 44 tests)
- Phase 3: COMPLETE ✓ (5/5 tasks, prototyper builds)
- Phase 4: COMPLETE ✓ (7/7 tasks, prototyper builds with RDSM init)
- Phase 5: COMPLETE ✓ (4/4 tasks, prototyper builds with trap routing)
- Phase 6: BLOCKED (requires NEMU hardware, 0/5 tasks)

Total: 53/58 tasks complete (5 blocked on hardware)

## Phase 5 Research: riscv crate interrupt/exception enum support

**Confirmed**: `riscv` crate v0.16.1 `Interrupt` enum only has codes 1,3,5,7,9,11. MSDEI (code 14) is NOT present → `try_into()` fails → `unsupported_trap(None)` panics.

**Phase 5 plan**: In `fast_handler` (trap/mod.rs), add raw mcause check BEFORE `try_into()`:
```rust
let raw_mcause = mcause::read().bits();
let is_interrupt = (raw_mcause >> 63) & 1 == 1;
let code = raw_mcause & 0x7FFFFFFFFFFFFFFF;
if is_interrupt && code == 14 {
    // MSDEI handling
    return handler::msdei_handler(ctx);
}
```

**MPT access faults**: `Exception::InstructionFault=1`, `LoadFault=5`, `StoreFault=7` ARE in the enum. Currently fall into `_` catch-all in `handle_exception`. Need to add explicit cases that call `MptFault::from_trap()`.
