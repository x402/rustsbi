pub mod boot;
pub mod handler;
pub mod helper;

use super::pmu::pmu_firmware_counter_increment;
use crate::fail::unsupported_trap;

use fast_trap::{FastContext, FastResult};
use riscv::interrupt::machine::{Exception, Interrupt};
use riscv::register::{
    mcause::{self, Trap},
    mepc, mip, mstatus,
};
use sbi_spec::pmu::firmware_event;

/// Fast trap handler for all trap.
pub extern "C" fn fast_handler(
    mut ctx: FastContext,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
) -> FastResult {
    // Save mepc into context
    ctx.regs().pc = mepc::read();

    // Check for MSDEI (Machine Supervisor Domain External Interrupt, mip bit 14)
    // before the standard try_into() — the riscv crate v0.16 Interrupt enum
    // does not include code 14, so try_into() would fail and panic.
    let raw_mcause = mcause::read().bits();
    let is_interrupt = (raw_mcause >> 63) & 1 == 1;
    let code = raw_mcause & 0x7FFFFFFFFFFFFFFF;
    if is_interrupt && code == rdsm::interrupt::MsdeiTrap::MSDEI_CODE {
        let a0 = ctx.a0();
        save_regs_and_handle_msdei(&mut ctx, &[a0, a1, a2, a3, a4, a5, a6, a7]);
        return ctx.restore();
    }

    let cause = match mcause::read().cause().try_into() {
        Ok(cause) => cause,
        Err(err) => {
            error!("Failed to parse mcause: {:?}", err);
            unsupported_trap(None)
        }
    };

    // Fast path for SBI calls
    if let Trap::Exception(Exception::SupervisorEnvCall) = cause {
        return handler::sbi_call_handler(ctx, a1, a2, a3, a4, a5, a6, a7);
    }

    // Flatten-save argument registers for non-SBI traps before dispatching
    let a0 = ctx.a0();
    ctx.regs().a = [a0, a1, a2, a3, a4, a5, a6, a7];

    match cause {
        Trap::Interrupt(interrupt) => handle_interrupt(ctx, interrupt),
        Trap::Exception(exception) => handle_exception(ctx, exception),
    }
}

/// Save registers and handle an MSDEI interrupt.
///
/// MSDEI (Machine Supervisor Domain External Interrupt) traps to M-mode
/// and must NOT be delegated to S-mode.  In Phase 1, only the host SID
/// (SIDN 0) exists, so we log the pending SIDs and clear the enable mask
/// to acknowledge.
fn save_regs_and_handle_msdei(ctx: &mut FastContext, regs: &[usize; 8]) {
    ctx.regs().a = *regs;
    handler::msdei_handler();
}

fn handle_interrupt(
    mut ctx: FastContext,
    interrupt: Interrupt,
) -> FastResult {
    match interrupt {
        Interrupt::MachineSoft => {
            handler::msoft_handler(ctx)
        }
        Interrupt::MachineTimer => {
            use crate::riscv::current_hartid;
            use crate::sbi::features::{Extension, hart_extension_probe};
            use crate::sbi::ipi;

            unsafe {
                riscv::register::mie::clear_mtimer();
            }
            if !hart_extension_probe(current_hartid(), Extension::Sstc) {
                ipi::clear_mtime();
                unsafe {
                    mip::set_stimer();
                }
            }
            ctx.restore()
        }
        Interrupt::MachineExternal => {
            handler::mext_handler(ctx)
        }
        _ => {
            error!("Unhandled interrupt: {:?}", interrupt);
            unsupported_trap(Some(Trap::Interrupt(interrupt)))
        }
    }
}

fn handle_exception(
    mut ctx: FastContext,
    exception: Exception,
) -> FastResult {
    match exception {
        // TODO: Handle InstructionMisaligned
        Exception::InstructionMisaligned => {
            error!("TODO: Unhandled InstructionMisaligned exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        // MPT access faults (InstructionFault=1, LoadFault=5, StoreFault=7)
        // trap to M-mode and must NOT be delegated to S-mode.
        // In Phase 1, the host MPT is permissive, so these should not trigger.
        // If they do, log the fault and stop the violating hart.
        Exception::InstructionFault => {
            if let Some(fault) = rdsm::fault::MptFault::from_trap(
                mcause::read().bits(),
                riscv::register::mtval::read(),
            ) {
                error!(
                    "RDSM: M-mode access fault (possibly MPT) at paddr=0x{:x}, stopping hart {} (instruction)",
                    fault.paddr(),
                    crate::riscv::current_hartid()
                );
                crate::fail::stop();
            }
            error!("Unhandled InstructionFault exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::LoadFault => {
            if let Some(fault) = rdsm::fault::MptFault::from_trap(
                mcause::read().bits(),
                riscv::register::mtval::read(),
            ) {
                error!(
                    "RDSM: M-mode access fault (possibly MPT) at paddr=0x{:x}, stopping hart {} (load)",
                    fault.paddr(),
                    crate::riscv::current_hartid()
                );
                crate::fail::stop();
            }
            error!("Unhandled LoadFault exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::StoreFault => {
            if let Some(fault) = rdsm::fault::MptFault::from_trap(
                mcause::read().bits(),
                riscv::register::mtval::read(),
            ) {
                error!(
                    "RDSM: M-mode access fault (possibly MPT) at paddr=0x{:x}, stopping hart {} (store)",
                    fault.paddr(),
                    crate::riscv::current_hartid()
                );
                crate::fail::stop();
            }
            error!("Unhandled StoreFault exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::IllegalInstruction => {
            pmu_firmware_counter_increment(firmware_event::ILLEGAL_INSN);
            if mstatus::read().mpp() == mstatus::MPP::Machine {
                panic!("Cannot handle illegal instruction exception from M-MODE");
            }
            ctx.continue_with(handler::illegal_instruction_handler, ())
        }
        // TODO: Handle Breakpoint
        Exception::Breakpoint => {
            error!("TODO: Unhandled Breakpoint exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::LoadMisaligned => {
            pmu_firmware_counter_increment(firmware_event::MISALIGNED_LOAD);
            ctx.continue_with(handler::load_misaligned_handler, ())
        }
        Exception::StoreMisaligned => {
            pmu_firmware_counter_increment(firmware_event::MISALIGNED_STORE);
            ctx.continue_with(handler::store_misaligned_handler, ())
        }
        _ => {
            error!("Unhandled exception: {:?}", exception);
            unsupported_trap(Some(Trap::Exception(exception)))
        }
    }
}
