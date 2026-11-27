//! GIF packets and the DMA channel that delivers them.
//!
//! The PCRTC registers in [`crate::gs`] control what gets *displayed*;
//! this module is how anything gets *drawn*. GS general registers
//! (`PRIM`, `RGBAQ`, `XYZ2`, `FRAME_1`, ...) are not memory-mapped --
//! they are addressed by index inside a packet handed to the Graphics
//! Interface, which the EE feeds over DMA channel 2.
//!
//! Only one packet shape is implemented: `PACKED` mode with a single
//! register descriptor of `A+D` (0x0E), where every data quadword is
//! `{ value, register_index }`. That one shape can set any GS general
//! register and issue any primitive, which is enough for everything
//! this project needs so far and avoids hand-rolling the other GIF
//! modes before anything needs them.
//!
//! Sources, read rather than recalled (see docs/progress.md):
//!   - `GIF_SET_TAG(NLOOP, EOP, PRE, PRIM, FLG, NREG)`, `GIF_FLG_PACKED`
//!     and `GIF_REG_AD` from ps2sdk `common/include/gif_tags.h`.
//!   - DMA channel 2 = GIF, and the CHCR/MADR/QWC addresses, from
//!     ps2sdk `ee/dma/src/dma.c` + `ee/dma/include/dma_registers.h`.

use core::sync::atomic::{compiler_fence, Ordering};

/// DMA channel 2's registers (`DMA_CHANNEL_GIF`).
const D2_CHCR: *mut u32 = 0x1000_a000 as *mut u32;
const D2_MADR: *mut u32 = 0x1000_a010 as *mut u32;
const D2_QWC: *mut u32 = 0x1000_a020 as *mut u32;

/// `DMA_SET_CHCR(DIR=1, MODE=0, ASP=0, TTE=0, TIE=0, STR=1, TAG=0)`:
/// transfer from memory, normal (non-chain) mode, start now. ps2sdk
/// also sets `TIE`, which raises an interrupt on a tag -- pointless
/// here since nothing installs a handler, so it's left off.
const CHCR_START_FROM_MEMORY: u32 = (1 << 0) | (1 << 8);
/// `STR`: set while a transfer is in progress, cleared by the DMAC.
const CHCR_STR: u32 = 1 << 8;

/// One 128-bit GIF word. The GIF consumes quadwords, and DMA requires
/// 16-byte alignment, so this is the unit everything here is built from.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Qword {
    pub lo: u64,
    pub hi: u64,
}

impl Qword {
    pub const ZERO: Qword = Qword { lo: 0, hi: 0 };

    /// A data quadword for an `A+D` packet: a value and the index of the
    /// GS register to put it in.
    pub const fn reg(register: u8, value: u64) -> Qword {
        Qword {
            lo: value,
            hi: register as u64,
        }
    }
}

/// `GIF_REG_AD` -- the register descriptor meaning "each quadword names
/// its own destination register".
const REG_AD: u64 = 0x0e;
/// `GIF_FLG_PACKED`.
const FLG_PACKED: u64 = 0x00;

/// Builds the GIFtag that introduces an `A+D` packet of `nloop`
/// register writes. `NREG = 1` with `REGS = A+D` in the tag's upper
/// half.
pub const fn tag_a_d(nloop: u16) -> Qword {
    const EOP: u64 = 1 << 15;
    const NREG_1: u64 = 1 << 60;
    Qword {
        lo: (nloop as u64 & 0x7fff) | EOP | (FLG_PACKED << 58) | NREG_1,
        hi: REG_AD,
    }
}

/// Hands `packet` to the GIF over DMA channel 2 and waits for the
/// transfer to finish.
///
/// `packet[0]` must be a GIFtag and the slice must be the whole packet,
/// tag included -- `QWC` counts every quadword in the stream.
///
/// # Safety
/// `packet` must be 16-byte aligned (guaranteed by [`Qword`]) and must
/// outlive the transfer. The caller must have flushed the data cache
/// first -- see the note below.
pub unsafe fn send(packet: &[Qword]) {
    // The DMAC reads physical RAM and knows nothing about the EE's
    // write-back data cache, so a packet that is still only in cache
    // transfers as stale garbage. The kernel's FlushCache writes it
    // back. (Our virtual addresses double as physical ones: the loader
    // identity-maps the region it loaded us into, which is the same
    // assumption ps2sdk makes when it passes a plain pointer to MADR.)
    crate::syscall::flush_data_cache();
    compiler_fence(Ordering::SeqCst);

    unsafe {
        core::ptr::write_volatile(D2_QWC, packet.len() as u32);
        core::ptr::write_volatile(D2_MADR, packet.as_ptr() as u32);
        core::ptr::write_volatile(D2_CHCR, CHCR_START_FROM_MEMORY);

        while core::ptr::read_volatile(D2_CHCR) & CHCR_STR != 0 {
            core::hint::spin_loop();
        }
    }
}
