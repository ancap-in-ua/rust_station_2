//! Drawing to the framebuffer: the smallest useful layer over
//! [`crate::gif`].
//!
//! Right now that means one operation -- fill the screen with a flat
//! colour -- but it establishes the shape the rest will follow: build an
//! `A+D` packet of GS register writes into a caller-owned, 16-byte
//! aligned buffer, then hand it to the GIF over DMA.
//!
//! Why the caller owns the buffer: DMA reads it directly out of RAM, so
//! it has to be a `static` (or otherwise long-lived, aligned allocation)
//! rather than something on the stack that may be gone or stale by the
//! time the DMAC reads it. Taking it as a parameter keeps that decision
//! where the lifetime is actually known.

use crate::gif::{self, Qword};
use crate::gs;

/// How many GS register writes [`build_fill`] emits.
pub const FILL_REGISTER_WRITES: u16 = 11;

/// Quadwords a [`build_fill`] packet occupies: the register writes plus
/// the GIFtag that introduces them.
pub const FILL_PACKET_QWORDS: usize = FILL_REGISTER_WRITES as usize + 1;

/// Builds a packet that sets up the drawing environment and fills the
/// whole screen with `color`.
///
/// The environment is re-sent every time rather than being set once and
/// left alone. It costs seven quadwords and removes a class of bug where
/// drawing depends on state some earlier packet happened to leave
/// behind — worth it until there's a reason to care about the bandwidth.
pub fn build_fill(
    buf: &mut [Qword; FILL_PACKET_QWORDS],
    width: u16,
    height: u16,
    color: (u8, u8, u8),
) {
    let (r, g, b) = color;
    let frame_width = (width / 64) as u8;

    buf[0] = gif::tag_a_d(FILL_REGISTER_WRITES);

    // --- drawing environment -------------------------------------
    // Draw into VRAM offset 0, which is what gs::init_display points
    // the display circuit at.
    buf[1] = Qword::reg(gs::GS_FRAME_1, gs::frame(0, frame_width, gs::PSMCT32, 0));
    // No depth buffer: mask Z writes and pair that with an ALWAYS
    // depth test below (the GS manual prohibits disabling the test).
    buf[2] = Qword::reg(gs::GS_ZBUF_1, gs::zbuf(0, gs::PSMCT32, true));
    // Zero offset, so vertex coordinates are screen coordinates.
    buf[3] = Qword::reg(gs::GS_XYOFFSET_1, gs::xyoffset(0, 0));
    buf[4] = Qword::reg(gs::GS_SCISSOR_1, gs::scissor(0, width - 1, 0, height - 1));
    // Take primitive settings from the PRIM register below, rather than
    // from a PRIM field in each GIFtag.
    buf[5] = Qword::reg(gs::GS_PRMODECONT, 1);
    buf[6] = Qword::reg(gs::GS_COLCLAMP, 1);
    buf[7] = Qword::reg(gs::GS_TEST_1, gs::test_depth_always());

    // --- the primitive -------------------------------------------
    buf[8] = Qword::reg(gs::GS_PRIM, gs::prim(gs::PRIM_SPRITE));
    buf[9] = Qword::reg(gs::GS_RGBAQ, gs::rgbaq(r, g, b, 0x80));
    // A sprite takes exactly two vertices: opposite corners. The second
    // XYZ2 write is what kicks the primitive.
    buf[10] = Qword::reg(gs::GS_XYZ2, gs::xyz2(0, 0, 0));
    buf[11] = Qword::reg(
        gs::GS_XYZ2,
        gs::xyz2(gs::fixed12_4(width), gs::fixed12_4(height), 0),
    );
}

/// Builds and sends a full-screen fill, waiting for the DMA to drain.
///
/// # Safety
/// Same requirements as [`gif::send`]: `buf` must outlive the transfer.
#[cfg(target_arch = "mips")]
pub unsafe fn fill_screen(
    buf: &mut [Qword; FILL_PACKET_QWORDS],
    width: u16,
    height: u16,
    color: (u8, u8, u8),
) {
    build_fill(buf, width, height, color);
    unsafe {
        gif::send(buf);
    }
}

/// PMODE for "just show read circuit 1's pixels": no merge-circuit
/// tricks, which is what you want once something has actually been
/// drawn into the framebuffer.
pub const fn pmode_show_circuit1() -> u64 {
    gs::pmode(true, false, false, false, false, 0x80)
}

const _: () = assert!(FILL_PACKET_QWORDS == 12);
