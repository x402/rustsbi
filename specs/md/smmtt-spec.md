# RISC-V Supervisor Domains Access Protection

**RISC-V SmMTT Task Group**

**Version v0.49, 2026-01-15: Draft**

---

> **Preamble**: This document is in the Development state. Assume everything can change. This draft specification will change before being accepted as standard, so implementations made to this draft specification will likely not conform to the future standard.

---

## Copyright and License

This specification is licensed under the Creative Commons Attribution 4.0 International License (CC-BY 4.0). The full license text is available at [creativecommons.org/licenses/by/4.0/](https://creativecommons.org/licenses/by/4.0/).

Copyright 2024 by RISC-V International.

## Contributors

Alvin Che-Chia Chang, Andy Chiu, Andy Dellow, Anup Patel, Dean Liberty, Deepak Gupta, Eckhard Delfs, Gregor Haas, Guerney Hunt, Jack Andrew, Krste Asanovic, Mark Hill, Nick Wood, Osman Koyuncu, Paul Elliott, Ricardo Ramirez, Ravi Sahita (Editor), Samuel Holland, Samuel Ortiz, Ved Shanbhogue, Wojchiech Ozga

## Glossary

| Term | Definition |
|------|-----------|
| **ABI** | Application binary interface. |
| **AIA** | RISC-V Advanced Interrupt Architecture interrupts. |
| **AP** | Application processors -- can support commodity OSes, hypervisors/VMMs and applications. May contain several processing units, on-chip caches, and other controllers. |
| **Attestation** | Process by which a relying party can assess the trustworthiness of a confidential computing environment based on verifying cryptographically-endorsed evidences from a hardware root-of-trust. |
| **Confidential Computing** | A computing paradigm that protects data-in-use by performing computation in a hardware-based, attested, execution environment. |
| **CoVE** | Confidential VM Extension -- the set of RISC-V ABI extensions that enables confidential computing for hardware VMs on RISC-V platforms. |
| **HW RoT** | Hardware Root of Trust -- the isolated hardware/software subsystem with an immutable ROM firmware and isolated compute/memory elements forming the TCB. |
| **Hypervisor** | Software running in HS-mode that manages VMs by virtualizing hart, guest physical memory and IO resources. |
| **IMSIC** | Incoming Message-signaled Interrupt Controller. |
| **MMIO** | Memory mapped I/O. |
| **MMU** | Memory management unit. |
| **MPT** | Machine-level Memory Protection Tables. |
| **Relying party** | An entity that uses attestation to assess the trustworthiness of an attester. |
| **Supervisor Domain (SD)** | A RISC-V privileged architecture extension to support isolation across more than one supervisor execution context. All host software elements (OS, VMM, hosted VMs) operate in a "hosting" supervisor domain. |
| **TCB** | Trusted Computing Base -- the hardware, software, and firmware elements trusted to protect confidentiality and integrity of workload data against a defined adversary model. |
| **TEE** | Trusted Execution Environment -- hardware and software mechanisms for creating attestable and isolated execution environments. |
| **Tenant workload** | All software elements owned and deployed by a single organization, hosted by a platform operator. |
| **VM** | An efficient, isolated duplicate of a real computer system -- resources accessible when virtualization mode is set to 1. |
| **VMM** | Virtual Machine Monitor (hypervisor). |

---

## 1. Introduction

### 1.1. Motivation and Goals

RISC-V privileged architecture defines execution mode for supervisor software called S-mode. S-mode software may optionally enable the Hypervisor extension to host VMs. Typically, there is a single supervisor domain of execution with access to all physical memory. This document describes **Supervisor Domain Access Protection** -- a RISC-V privileged architecture extension to support physical address space (memory and devices) isolation for more than one supervisor domain.

Supervisor domains enable trusted execution use cases for RISC-V platforms, and may also be used to reduce the supervisor Trusted Computing Base (TCB), with differentiated access to memory and other platform resources (e.g., Confidential Computing, TEE Security Services, Secure Devices).

Tenant workloads on multi-tenant platforms rely on hardware-based isolation primitives managed by host/privileged software. Traditional host software has unfettered access to system memory, devices and hardware isolation mechanisms (MMU, PMP). Additional threats originate from deployment models (e.g., physical threat vectors for embedded platforms). Hence, isolated supervisor domains are desired to express differentiated trust models and secure access to platform resources.

A **supervisor domain** is associated with a set of physical address regions isolated from other supervisor domains, with only the **Root Domain Security Manager (RDSM)** having access to all physical address space. A **supervisor domain identifier (SDID)** is associated with the hart operating in the context of a supervisor domain to facilitate physical address protection fences on a per-SD basis.

**Use cases** for supervisor domain isolation:
- A trusted execution environment domain isolating security services/applications
- A confidential computing domain enforcing confidentiality and integrity for workload data-in-use
- A host (operator) domain managing platform resources
- A service-provider domain with exclusive device access

### 1.2. Memory Isolation -- Theory of Operation (Informative)

Supervisor Domain Access Protection extensions are used by M-mode RDSM to program access policies for supervisor domain operation. The **Smmpt** extension enables the RDSM to program permissions for physically-addressed memory (or device-mapped regions) by a hart/device operating within a supervisor domain.

The **MPT checker** looks up the MPT using the physical address as an index to retrieve access permissions for the supervisor domain. For a load, the physical address must be readable; for a store, it must also be writable -- else a fault is reported. An MPT access violation always traps to M-mode RDSM. The MPT checker may be implemented as an MMU extension in the hart, and/or in the IO interconnect.

Read and Write permissions for memory are derived from the page table, the PMP, and the MPT -- **an access is allowed only when all protection mechanisms allow the access**.

MPT must support both static and run-time configurability. Run-time reconfiguration may require TLB/cache flushes enforced by the RDSM. Memory sharing between supervisor domains is achieved by making the physical memory region accessible to the relevant supervisor domains via the MPT structure.

---

## 2. Summary of Extensions for Supervisor Domain Access Protection

The following normative architecture extensions are defined:

| Extension | Chapter | Description |
|-----------|---------|-------------|
| **Smsdid** | 3 | Interface to program the active supervisor domain for a hart via the SDID CSR. Independent or combined with other extensions. |
| **Smmpt** | 4 | Set access permissions for memory regions/pages per supervisor domain. Dynamic permission changes via Machine-level Memory Protection Tables (MPT). When implemented, MPT and e(PMP) are always active. |
| **IO-MPT** | 5 | Non-ISA extension for IO interconnect to associate IOMMU and devices with a supervisor domain. |
| **Smsdia** | 6 | Assignment of IMSIC interrupt files or APLIC domains to a supervisor domain. |
| **Smsdedbga** | 7 | Controls to indicate if external debug is allowed for a supervisor domain. |
| **Smsdetrca** | 7 | Controls to indicate if external trace is allowed for a supervisor domain. |
| **Smsqosid / CBQRI** | 8 | Interface for RDSM to enforce QoS resource isolation between supervisor domains. |

---

## 3. Smsdid: Supervisor Domain Identifier and Protection Register

Smsdid defines an interface to program supervisor-domain-related configuration via M-mode CSRs `msdcfg` and `mmpt`. When Smsdid is implemented, both CSRs must be implemented; `msdcfg` may be read-only zero if no optional extensions are implemented.

The SDID is a local identifier for the hart and must **not** be used to tag downstream transactions that are globally visible.

> **Note**: For most typical usages, the SDID will remain stable over the life of the supervisor domain. However, the SDID can be changed at the cost of flushing any prior cached state associated with the SDID.

### 3.1. Machine-level Memory Protection Tables (`mmpt`)

The `mmpt` register is an MXLEN-wide read/write machine-mode register (CSR 0x382) controlling page-based physical address space protection.

**Fields:**
- **MODE** (WARL) -- selects the memory protection scheme
- **SDID** (WARL) -- supervisor domain identifier (SDIDLEN bits, max SDIDMAX=6)
- **PPN** (WARL) -- root page of the memory protection tables

**MODE encodings for MXLEN=32:**

| Value | Name | Description |
|-------|------|-------------|
| 0 | Bare | No page-based memory protection. SDID, PPN must be zeros. |
| 1 | Smmpt34 | Page-based protection for up to 34-bit physical address spaces. |
| 2 | -- | Reserved for future standard use. |
| 3 | -- | Designated for custom use. |

**MODE encodings for MXLEN=64:**

| Value | Name | Description |
|-------|------|-------------|
| 0 | Bare | No page-based memory protection. SDID, PPN must be zeros. |
| 1 | Smmpt43 | Page-based protection for up to 43-bit physical address spaces. |
| 2 | Smmpt52 | Page-based protection for up to 52-bit physical address spaces. |
| 3 | Smmpt64 | Page-based protection for up to 64-bit physical address spaces. |
| 4-13 | -- | Reserved for future standard use. |
| 14-15 | -- | Designated for custom use. |

When `mmpt.MODE` is `Smmpt64`, the root page must be aligned to a 32 KiB boundary.

The `mmpt` register is active for access type permission lookup unless the effective privilege mode is M.

Writing `mmpt` does **not** imply ordering constraints between updates and subsequent translations/protection checks. If an MPT structure has been modified or an SDID reused, `MFENCE.PA` may be needed.

### 3.2. Machine Memory Protection Fence Instruction (MFENCE.PA)

```
MFENCE.PA rs1(PADDR), rs2(SDID)
func7=MFENCE.PA | rs2(SDID) | rs1(PADDR) | func3=PRIV | rd=0 | opcode=SYSTEM
```

MFENCE.PA synchronizes updates to memory protection tables. **Only valid in M-mode.**

Behavior based on rs1 and rs2:

| rs1 | rs2 | Effect |
|-----|-----|--------|
| x0 | x0 | Fence all reads/writes to any level of MPTs for all SD address spaces. |
| x0 | !=x0 | Fence all reads/writes to any level of the MPT for the SD identified by rs2. |
| !=x0 | x0 | Fence reads/writes to leaf MPT entries for the physical address in rs1, for all SD address spaces. |
| !=x0 | !=x0 | Fence reads/writes to leaf MPT PTEs for the physical address in rs1, for the SD in rs2. |

> **Note**: A simpler implementation may ignore rs1 and/or rs2 and always perform a global fence for all SDs.

### 3.3. M-mode Supervisor Domain Fine-Grain Invalidation Instruction (MINVAL.PA)

When Svinval and Smsdid are both implemented, `MINVAL.PA` must be implemented. It supports batching of MPT invalidations, analogous to SINVAL.VMA.

```
MINVAL.PA rs1(PADDR), rs2(SDID)
func7=MINVAL.PA | rs2(SDID) | rs1(PADDR) | func3=PRIV | rd=0 | opcode=SYSTEM
```

MINVAL.PA is only ordered against `SFENCE.W.INVAL` and `SFENCE.INVAL.IR`. When executed in order: `SFENCE.W.INVAL`, `MINVAL.PA`, `SFENCE.INVAL.IR` has the same effect as `MFENCE.PA` with the same rs1/rs2. **Only valid in M-mode.**

### 3.4. Extension to Behavior of Memory-Management Fences

When Smsdid is implemented, the behavior of `SFENCE.VMA`, `HFENCE.GVMA`, `HFENCE.VVMA` is affected. When Svinval is also implemented, `SINVAL.VMA`, `HINVAL.GVMA`, `HINVAL.VVMA` are also affected.

The ASID and VMID arguments correspond to ASIDs/VMIDs active within the current supervisor domain (identified by `mmpt.SDID`). The SDID augments the ASID and VMID to form **effective identifiers**:
- Effective S/HS-level ASID = SDID + S/HS-level ASID
- Effective VS-level ASID = SDID + VS-level ASID
- Effective VMID = SDID + VMID

### 3.5. Machine Supervisor Domain Configuration (`msdcfg`)

`msdcfg` is an MXLEN-wide machine-mode read/write register (CSR 0x74E) holding configurations for a supervisor domain.

**Fields:**

| Bits | Field | Description | Specified by |
|------|-------|-------------|-------------|
| 31:28 | SML | Supervisor MCID length | Smsdqosid |
| 27:24 | SRL | Supervisor RCID length | Smsdqosid |
| 23 | SSMM | Supervisor srmcfg MCID mode | Smsdqosid |
| 22 | SSRM | Supervisor srmcfg RCID mode | Smsdqosid |
| 21:16 | -- | WPRI | -- |
| 15:8 | -- | WPRI | -- |
| 7 | SETA | Supervisor external trace allow | Smsdetrca |
| 6 | SEDA | Supervisor external debug allow | Smsdedbga |
| 5:0 | SIDN | Supervisor interrupt domain number | Smsdia |

If an extension is not implemented, the corresponding bits are read-only zero.

---

## 4. Machine-mode Memory Protection Table (MPT)

MPT is a radix-tree table providing read, write, and execute permissions for implicit and explicit memory accesses by effective privilege modes less than M.

**Defined MPT extensions:**

| Extension | Address bits | Levels | Page (MPTE) size | RV | Depends on |
|-----------|-------------|--------|-----------------|-----|-----------|
| Smmpt34 | 34-bit | 2 | 4 bytes | RV32 | Smsdid |
| Smmpt43 | 43-bit | 3 | 8 bytes | RV64 | Smsdid |
| Smmpt52 | 52-bit | 4 | 8 bytes | RV64 | Smsdid |
| Smmpt64 | 64-bit | 5 | 8 bytes (root: 32 KiB) | RV64 | Smsdid |

### 4.1. Smmpt34: Page-Based Memory Protection System

The RV32 MPT provides memory access type permissions for pages in a 34-bit supervisor physical address (SPA) space. Two-level memory protection table.

**SPA partitioning:** `pn[1] (9 bits) | pn[0] (10 bits) | range offset (15 bits)`

- Root table: 2^9 MPTEs; linked tables: 2^10 MPTEs
- Each MPTE is 4 bytes; tables aligned to page boundary

**Non-leaf MPTE:** `PPN[31:10] | Reserved[9:2] | L=0 | V=1`

**Leaf MPTE (non-NAPOT):** Contains eight XWR[7:0] tuples (3 bits each) for 8 pages + Reserved + N=0, L=1, V=1

**Leaf MPTE (NAPOT):** Reserved + G[11:10] + Reserved + XWR[10:8] + Reserved + N=1, L=1, V=1. Encodings 0-5 and 7-15 of G are reserved for Smmpt34.

**XWR permission encoding:**

| X | W | R | Meaning |
|---|---|---|---------|
| 0 | 0 | 0 | No access |
| 0 | 0 | 1 | Read-only |
| 0 | 1 | 0 | Reserved |
| 0 | 1 | 1 | Read-write |
| 1 | 0 | 0 | Execute-only |
| 1 | 0 | 1 | Read-execute |
| 1 | 1 | 0 | Reserved |
| 1 | 1 | 1 | Read-write-execute |

> **Note**: The V bit indicates MPTE validity. A change from Invalid to Valid eventually becomes visible within bounded time without explicit fence. Other MPTE changes require MFENCE.PA or MINVAL.PA.

**Supported page sizes:** 4 KiB, 4 MiB (NAPOT with G=6)

#### 4.1.1. MPT Access Type Permissions Lookup Process

1. Let `a = mmpt.ppn * PAGESIZE`, `i = LEVELS-1` (for Smmpt34: PAGESIZE=2^12, LEVELS=2). `mmpt` must be active (effective privilege != M).
2. Let `mpte` = MPTE at address `a + pa.pn[i] * MPTESIZE` (Smmpt34: MPTESIZE=4). If PMA/PMP violation -> access-fault.
3. If `mpte.V=0` or reserved bits set or `mpte.L=0 && mpte.N=1` -> access-fault.
4. If `mpte.L=0` -> non-leaf: `i=i-1`. If `i<0` -> access-fault. Else `a = mpte.PPN * PAGESIZE`, goto step 2.
5. If `mpte.N=0` -> non-NAPOT leaf: extract `XWR[pi]` where `pi` = NUMPGINRANGE MSBs of `pa.pn[i-1]` (Smmpt34: NUMPGINRANGE=3).
6. If `mpte.N=1` -> NAPOT leaf: `XWR = mpte.XWR`. Check G encoding validity.
7. If access not allowed by XWR -> access-fault; else allowed.

**Key constraints:**
- MXR/HLVX cannot override MPT permissions
- Zicfiss shadow stack requires read-write permission in MPT
- Implicit MPT accesses are checked by PMP/Smepmp as implicit M-mode accesses

### 4.2. Smmpt43: Page-Based 43-bit Memory Protection System

RV64, 43-bit address space, three-level table. Follows Smmpt34 scheme with these differences:

**SPA partitioning:** `pn[2] (9) | pn[1] (9) | pn[0] (9) | range offset (16)`

- Tables: 2^9 MPTEs, each 8 bytes, page-aligned
- Leaf MPTE provides permissions for **sixteen** pages (NUMPGINRANGE=4)
- LEVELS=3, MPTESIZE=8

**Supported page sizes:** 4 KiB, 2 MiB, 1 GiB

G encoding 4 supports caching a 2 MiB or 1 GiB range. Encodings 0-3 and 5-15 of G are reserved.

### 4.3. Smmpt52: Page-Based 52-bit Memory Protection System

RV64, 52-bit address space, **four-level** table. Adds one level to Smmpt43.

**SPA partitioning:** `pn[3] (9) | pn[2] (9) | pn[1] (9) | pn[0] (9) | range offset (16)`

- LEVELS=4, MPTESIZE=8, NUMPGINRANGE=4
- MPTE formats identical to Smmpt43

**Supported page sizes:** 4 KiB, 2 MiB, 1 GiB, 512 GiB

### 4.4. Smmpt64: Page-Based 64-bit Memory Protection System

RV64, 64-bit address space, **five-level** table. Adds one level to Smmpt52.

**SPA partitioning:** `pn[4] (12) | pn[3] (9) | pn[2] (9) | pn[1] (9) | pn[0] (9) | range offset (16)`

- **Root table**: 2^12 MPTEs -> 32 KiB, aligned to 32 KiB boundary
- Non-root tables: 2^9 MPTEs, page-aligned
- LEVELS=5, MPTESIZE=8, NUMPGINRANGE=4
- MPTE formats identical to Smmpt52

**Supported page sizes:** 4 KiB, 2 MiB, 1 GiB, 512 GiB, 256 TiB

---

## 5. I/O MPT Checker

Supervisor domains may be granted control over DMA-capable devices. The IO-MPT extension ensures DMA from devices and associated IOMMUs adhere to the MPT access protections of their supervisor domain.

The I/O subsystem provides two functions:
- **Supervisor Domain Classifier (SDCL)**: determines the appropriate MPT for a DMA request
- **MPT Check**: applies Smmpt access controls to memory regions accessed by DMA

### 5.1. Placement of I/O MPT Checker

The I/O Bridge sits between I/O devices and the system interconnect, processing DMA transactions. It invokes the I/O MPT Checker via SDID request/completion interfaces and MPT check request/completion interfaces.

The MPT permission lookup process used by I/O MPT Checker is **identical** to the CPU MMU process specified in Section 4.1.1.

**MPT entry update rules for I/O MPT Checker:**
- Software must use a single atomic store to update all/part of an MPT PTE
- If updating makes a field inconsistent with another, first set V=0 and use MPTINVAL
- I/O MPT Checker is not required to immediately observe updates; software must use MPTINVAL to synchronize

**Key properties:**
- I/O MPT Checker must not cache entries with V=0
- Provides a memory-mapped register programming interface
- Little-endian byte order (even for big-endian-only systems)
- Single-copy atomic access: at least 32-bit (RV32 systems) or 64-bit (RV64 systems)

### 5.2. Handling I/O MPT Checker Violations

A transaction is **aborted** if:
- I/O MPT Checker is in Off mode, or Bare mode and transaction is TEE-associated
- On mode but no matching rule for the transaction
- Memory targeted is disallowed by MPT
- MPT access is disallowed by PMA/PMP
- MPT returns corrupted (poisoned) data

**Abort handling:**
- **IOMMU-initiated**: signaled to the IOMMU (details implementation-defined)
- **Write**: may be discarded; protocol-specific response (e.g., AXI SLVERR, PCIe posted -> no response)
- **Read**: completion returned with implementation-defined data (usually all 0 or all 1) and status code (AXI SLVERR, PCIe UR/CA)

### 5.3. I/O MPT Checker Register Interface

Memory-mapped, 8-byte aligned physical address. 4-byte accesses must be single-copy atomic.

| Offset | Name | Size | Description |
|--------|------|------|-------------|
| 0 | capabilities | 4 | Capabilities (read-only) |
| 4 | status | 4 | Status |
| 8 | control | 4 | Control |
| 12 | command | 4 | Command |
| 16 | data1 | 8 | Input/Output data 1 |
| 24 | data2 | 8 | Input/Output data 2 |

**Reset values:**
- `status.BUSY` and `status.CODE`: 0
- `control.MODE`: recommended Off
- MPT PTE caches: no valid entries after reset
- All other fields: UNSPECIFIED

### 5.4. Capabilities Register

Read-only. The `VER` field holds the specification version: upper nibble = major, lower nibble = minor (e.g., v1.0 -> 0x10).

### 5.5. Status Register

| Field | Description |
|-------|-------------|
| BUSY | Set to 1 when command is executing. Software must verify BUSY=0 before writing command/control/data. |
| CODE | Operation result (when BUSY=0): |

| CODE | Status | Description |
|------|--------|-------------|
| 0 | -- | Reserved |
| 1 | Success | Operation completed successfully |
| 2 | Invalid OP | Invalid operation requested |
| 3 | Invalid RULEID | Invalid rule ID |
| 4 | Invalid SDID | Invalid supervisor domain ID |
| 5 | Invalid operands | Illegal or invalid operand encodings |
| 6-127 | -- | Reserved for future standard use |
| 128-255 | -- | Custom use |

### 5.6. Control Register

The `MODE` field controls I/O MPT Checker operation:

| Mode | Encoding | Description |
|------|----------|-------------|
| Off | 0 | No inbound memory transactions allowed |
| Bare | 1 | No memory protection (non-TEE transactions allowed; TEE-associated aborted) |
| On | 2 | I/O MPT Checker is operational |
| Reserved | 3-13 | Reserved for future standard use |
| Custom | 14-15 | Designated for custom use |

**Mode transitions:**
- When changing to Off: in-flight transactions are processed with old config or aborted; all prior transactions committed to global ordering point
- When On: must first change to Off without modifying other fields
- When Bare: allow non-TEE transactions, abort TEE-associated

Writes to control may be asynchronous; `status.BUSY` is set during processing.

### 5.7. Command Register

Operations are initiated by writing the `command` register with the appropriate `OP` field:

| OP | Operation | Description |
|----|-----------|-------------|
| 0 | -- | Reserved |
| 1 | IOFENCE | Commit all previous device read/write requests to global ordering point |
| 2 | SET_SDCL_ENTRY | Configure a SDCL rule |
| 3 | GET_SDCL_ENTRY | Read SDCL rule configuration |
| 4 | SET_SDCFG_ENTRY | Configure MPT and QoS ID parameters for an SD |
| 5 | GET_SDCFG_ENTRY | Read SD configuration |
| 6 | MPTINVAL | Synchronize MPT stores with I/O MPT Checker |
| 7-127 | -- | Reserved |
| 128-255 | -- | Custom |

Before requesting an operation, software must program `data1` and `data2` as required. Error checking order (SDID, RULEID, other operands) is UNSPECIFIED.

#### 5.7.1. SET_SDCL_ENTRY

Configures an SDCL rule to classify DMA requests, determining SDID and IOMMU_ID. Uses `command` and `data1` registers; `data2` is ignored.

**Fields in data1:**

| Field | Description |
|-------|-------------|
| SRC_IDT | Source identifier type: 0=None, 1=Device ID, 2=PCIe IDE stream+segment ID, 3-7=Reserved, 8-15=Custom |
| SRC_IDM | Source ID matching mode: 0=Reserved, 1=TOR (Top-Of-Range), 2=Unary (exact match), 3=NAPOT (power-of-two range) |
| SRC_ID | Source identifier value |
| TEE_FLT | TEE filter: 0=Match both, 1=TEE-associated only, 2=Non-TEE only, 3=Reserved |
| IOMMU_ID | IOMMU instance for address translation (ignored when no IOMMU) |

If multiple rules match, implementation may use any one. If no rule matches, the I/O Bridge is notified.

#### 5.7.2. GET_SDCL_ENTRY

Reads SDCL rule configuration by RULEID. Returns configuration in `data1`. `data2` is UNSPECIFIED.

#### 5.7.3. SET_SDCFG_ENTRY

Configures MPT and QoS parameters for a supervisor domain. Uses `command`, `data1`, and `data2`.

**MPT_MODE encodings (MXL=0, i.e., RV64):**

| Value | Name | Description |
|-------|------|-------------|
| 0 | Bare | No page-based memory protection |
| 1 | Smmpt43 | Up to 43-bit physical address |
| 2 | Smmpt52 | Up to 52-bit physical address |
| 3 | Smmpt64 | Up to 64-bit physical address |
| 4-13 | -- | Reserved |
| 14-15 | -- | Custom |

**MPT_MODE encodings (MXL=1, i.e., RV32):**

| Value | Name | Description |
|-------|------|-------------|
| 0 | Bare | No page-based memory protection |
| 1 | Smmpt34 | Up to 34-bit physical address |
| 2-13 | -- | Reserved |
| 14-15 | -- | Custom |

Other fields: `PPN` (root MPT page), `MBE` (endianness), and QoS-related fields (`SRL`, `SML`, `SSRM`, `SSMM`, `S-RCID`, `S-MCID`, `SQRID`).

**Effective RCID/MCID determination:**
```
SRL_MASK = (1 << SRL) - 1
if SSRM == 0:
    effective-RCID = (S-RCID & ~SRL_MASK) | (RCID-value & SRL_MASK)
else:
    if ((RCID-value & ~SRL_MASK) | SRL_MASK) != ((1 << RCIDLEN) - 1):
        effective-RCID = RCID-value
    else:
        effective-RCID = <unspecified but legal value>
```
(Analogous for MCID with SML/SSMM.)

#### 5.7.4. GET_SDCFG_ENTRY

Reads configuration for the SD identified by SDID. Returns in `data1` and `data2`.

#### 5.7.5. MPTINVAL

Ensures MPT stores are observed by I/O MPT Checker before subsequent implicit reads. Uses `command` and `data1`.

| PPNV | SDIDV | Operation |
|------|-------|-----------|
| 0 | 0 | Invalidate all cached MPT info for all SD address spaces |
| 0 | 1 | Invalidate cached MPT info for the specified SDID |
| 1 | 0 | Invalidate cached MPT info for the PPN address range, all SDs |
| 1 | 1 | Invalidate cached MPT info for the PPN range and specified SDID |

When `S=1`, the range is NAPOT-encoded using the low-order bits of PPN (up to first zero bit). If `S=0`, range is 4 KiB.

#### 5.7.6. IOFENCE

Ensures all previous device read/write requests are committed to a global ordering point observable by all harts, IOMMUs, and devices.

If configurations (SDCL rules, SD configs) are modified, an IOFENCE after the update guarantees the I/O MPT Checker observes the new configurations. If MPT mode/PPN changed, MPTINVAL with PPNV=0 must precede IOFENCE.

### 5.8. Treatment of Device-Side Address Translation Caches

Some devices provide a device-side ATC (DevATC) reducing IOATC thrashing. When PCIe ATS is supported:

- **PCIe ATS Translation Requests**: IOMMU performs two-stage translation. If SPA returned -> I/O MPT Checker invoked on entire translated range. If GPA returned -> not invoked.
- **Translated Requests**: IOMMU determines authorization; I/O MPT Checker invoked on SPA range.

---

## 6. Supervisor Domain Interrupt Assignment

Interrupts during SD execution trap to RDSM in M-mode by default. The RDSM may delegate certain interrupts (timer, LCOFI) via `mideleg`. External interrupts from devices assigned to SDs can be directly assigned via IMSIC/APLIC extensions.

### 6.1. IMSIC Extension for Supervisor Domains

IMSIC is extended to support multiple supervisor interrupt domains, each consisting of:
- A supervisor-level interrupt file
- Optionally, one or more guest interrupt files

Each supervisor interrupt domain is identified by a **SIDN** (Supervisor Interrupt Domain Number).

Address calculations for supervisor-level interrupt files use constants B, C, D (from AIA spec) and new constant I (>= k + D, where k = bits for hart number).

### 6.2. APLIC Extension for Supervisor Domains

APLIC must support multiple supervisor-level interrupt domains. For MSI delivery mode, `smsiaddrcfgh` is extended with:
- **DXS** (Domain Index Shift): bits 28:24, relates to constant I as DXS = (I - 12)
- **DXW** (Domain Index Width): bits 18:16

### 6.3. Smsdia: Standard Extension for Supervisor Domain Interrupt Assignment

Adds `msdcfg.SIDN` to select a supervisor interrupt domain as source of supervisor-level and guest external interrupts.

Introduces `msideip`/`msideie` CSRs and the **MSDEI** (Machine Supervisor Domain External Interrupt).

#### 6.3.1. Supervisor Interrupt Domain Number (SIDN)

WLRL field (0 to max implemented SIDN). When SIDN is not an implemented domain:
- `mip.SEIP` = 0
- `siselect` designates inaccessible registers -> illegal instruction
- `stopei` -> illegal instruction
- `hstatus.VGEIN` = 0, `hgeip`/`hgeie` = 0

When SIDN selects a valid IMSIC domain:
- SEIP from the selected domain's supervisor-level interrupt file
- `siselect`/`stopei` operate on that file's registers
- `hgeip` reflects guest interrupt files of selected domain

When SIDN selects a valid APLIC domain:
- SEIP from the selected APLIC supervisor interrupt domain

#### 6.3.2. Hypervisor Guest External Interrupt Registers

When H extension is implemented, `hgeip`/`hgeie` are internally per-SIDN arrays, accessed based on `msdcfg.SIDN`.

#### 6.3.3. msideip/msideiph (CSR 0xF4F / 0xF5F)

64-bit read-only machine-mode register. Each bit i summarizes external interrupts pending in SID i.

#### 6.3.4. msideie/msideieh (CSR 0x74F / 0x75F)

64-bit read-write machine-mode register. Selects which supervisor interrupt domains cause MSDEI. Does not affect SEIP/SGEIP from the active domain.

#### 6.3.5. Machine and Supervisor Interrupt Registers

**MSDEI** assigned to bit 14 in `mip`/`mie`/`sip`/`sie`:
- `mip.MSDEIP` = read-only: set if `(msideip & msideie) != 0`
- Cannot be delegated to VS-mode (bit 14 of `hideleg` = read-only zero)

**Interrupt priority** (decreasing): MEI, MSI, MTI, **MSDEI**, SEI, SSI, STI, SGEI, VSEI, VSSI, VSTI, LCOFI

### 6.4. Smgeien/Ssgeien: Standard Extension for Machine Guest External Interrupt Enables

Enables access to a subset of guest interrupt files within a supervisor interrupt domain. Depends on Smsdia, Smaia, and H extension.

Introduces **mgeien** register (CSR 0x74D):
- **GIF** field: each bit enables access to the corresponding guest interrupt file
- **A** bit: when 1, `msideip` accumulates all external interrupts; when 0, only subset determined by mgeien.GIF

Number of writable bits = MGEILEN (max GEILEN across all supervisor interrupt domains).

When `Ssgeien` is implemented:
- Guest external interrupts in `hgeip`/`hgeie` may not be contiguous
- Supervisor-level and guest interrupt files may not be contiguous in memory
- Supervisor-level interrupt file may not be available even when guest files are

---

## 7. Supervisor Domain External Trace and Debug Allow Controls

Two extensions enabling RDSM to enforce isolation during external debug/trace:

- **Smsdedbga**: `msdcfg.SEDA` bit -- external debug allow
- **Smsdetrca**: `msdcfg.SETA` bit -- external trace allow

These only affect debug/trace by an external agent. Self-hosted debug/trace is managed by RDSM directly.

> **Note**: SEDA/SETA values shall be obtained from configuration information reflected in the attestation report. These configurations **allow** but do not **enable** external debug/trace.

### 7.1. Smsdedbga: External Debug Allowed Control

- When M-mode external debug is **enabled**: all SDs may be debugged regardless of SEDA.
- When M-mode external debug is **disabled**:
  - SEDA=0: disallow all external debug access
  - SEDA=1: permit external debug for privilege modes less than M

**Requirements:**
- External debug of M-mode can be enabled only by HW RoT
- RDSM must be able to context-switch all triggers even when d-mode=1

### 7.2. Smsdetrca: External Trace Allowed Control

- When M-mode external trace is **enabled**: all SD activity may be traced regardless of SETA.
- When M-mode external trace is **disabled**:
  - SETA=0: disallow all external trace
  - SETA=1: permit external trace for privilege modes less than M, subject to:
    - External tracing stops upon entry into Debug Mode or M-mode

**Requirement:** External trace of M-mode shall be enabled only by HW RoT.

---

## 8. Capacity and Bandwidth QoS Register Interface (CBQRI) for Supervisor Domains

### 8.1. Introduction to CBQRI and Ssqosid

CBQRI specifies mechanisms to: (a) associate QoS identifiers with requests, (b) memory-mapped register interfaces for capacity allocation/monitoring, (c) memory-mapped register interfaces for bandwidth allocation/monitoring.

Ssqosid provides `srmcfg` CSR to configure QoS Identifiers for hart requests.

### 8.2. Security Objective

Resource accesses and allocations from an SD or RDSM must not be observable/influenced by entities outside their TCB.

Required capabilities:
- Multiple QoS Register Interfaces (QRIs)
- Associating QRID with SD/RDSM requests
- Smsdqosid for partitioning RCID/MCID per QRI
- Smqosid for M-mode QoS identifiers

### 8.3. Smqosid: Machine-Mode QoS Identifiers

Introduces **mrmcfg** register (CSR 0x381) -- MXLEN-wide read/write machine-mode register configuring:
- **QRID**: QoS Register Interface ID
- **RCID**: Resource Control ID
- **MCID**: Monitoring Counter ID

These accompany each request with effective privilege mode = M to shared resource controllers. When Smqosid is implemented, `srmcfg` applies only to requests with effective privilege < M.

### 8.4. Smsdqosid: Supervisor Domain QoS Identifiers

Enables multiple SDs sharing capacity/bandwidth controllers with mediated access. Introduces **mnrmcfg** register (CSR 0x781) containing QRID, RCID, MCID for requests from effective privilege < M.

Also introduces `msdcfg` fields: **SRL**, **SML**, **SSRM**, **SSMM**.

**Effective RCID/MCID calculation:**
```
SRL_MASK = (1 << SRL) - 1
RCID = (SSRM == 0) ? (mnrmcfg.RCID & ~SRL_MASK) | (srmcfg.RCID & SRL_MASK)
                     : srmcfg.RCID

SML_MASK = (1 << SML) - 1
MCID = (SSMM == 0) ? (mnrmcfg.MCID & ~SML_MASK) | (srmcfg.MCID & SML_MASK)
                     : srmcfg.MCID
```

When SSRM=0: SD selects from low SRL bits; high bits from mnrmcfg.
When SSRM=1: SD directly selects from full range (0 to 2^RCIDLEN - 2^SRL - 1).

### 8.5. Supervisor Domain QoS Register Interfaces (QRI)

Each controller may provide multiple QRIs. The **Machine-mode QRI (MQRI)** supports `CONFIG_QRI_LIMIT` (OP=4) to partition resources among QRIs.

**Capacity reservation:** Uses `cc_block_mask` -- must be contiguous run of 1s; NCBLKS of target QRI updated on success.

**Bandwidth reservation:** Uses `Rbwb` and `Mweight`. When Mweight nonzero -> proportional share of unused bandwidth among QRIs. Two-level weighted sharing: QRIs contend for unused bandwidth proportionally by Mweight, then distribute to their RCIDs by configured weights.

---

## Bibliography

| Ref | Source | URL |
|-----|--------|-----|
| [1] | RISC-V Advanced Interrupt Architecture | github.com/riscv/riscv-aia |
| [2] | RISC-V Confidential VM Extension | github.com/riscv-non-isa/riscv-ap-tee |
| [3] | RISC-V ISA Manual Vol. II: Privileged Architecture | github.com/riscv/riscv-isa-manual |
| [4] | Common Terminology for Confidential Computing | confidentialcomputing.io |
| [5] | PCI-SIG specifications | pcisig.com/specifications |
| [6] | RISC-V IOMMU specification | github.com/riscv-non-isa/riscv-iommu |
| [7] | RISC-V Debug Specification | github.com/riscv/riscv-debug-spec |
| [8] | Efficient Trace for RISC-V v2.0.2 | github.com/riscv-non-isa/riscv-trace-spec |
| [9] | RISC-V External Debug Security v0.0 | github.com/riscv-non-isa/riscv-external-debug-security |
| [10] | TCG Glossary | trustedcomputinggroup.org |
| [11] | RISC-V CBQRI | github.com/riscv-non-isa/riscv-cbqri |
| [12] | RISC-V QoS Identifiers | github.com/riscv/riscv-ssqosid |
