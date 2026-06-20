//! Flush-to-zero guard for the audio thread.
//!
//! Denormal floats are easy to generate in the many IIR / feedback loops this
//! plugin runs — EQ biquads, reverb combs and allpasses, tape/delay feedback
//! paths — especially while a long tail decays toward (but never reaches) zero.
//! On x86 the FPU handles denormals in microcode, which can spike CPU by an
//! order of magnitude. Setting the Flush-To-Zero and Denormals-Are-Zero bits in
//! `MXCSR` makes SSE math treat denormals as zero, which is the right trade-off
//! for audio and covers every loop at once.
//!
//! The guard restores the previous `MXCSR` on drop so the host's audio thread is
//! left exactly as we found it.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub struct FlushDenormals {
    previous_csr: u32,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl FlushDenormals {
    /// Flush-To-Zero (bit 15) and Denormals-Are-Zero (bit 6).
    const FTZ_DAZ: u32 = 0x8000 | 0x0040;

    #[inline]
    pub fn new() -> Self {
        let previous_csr = read_mxcsr();
        write_mxcsr(previous_csr | Self::FTZ_DAZ);
        Self { previous_csr }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Drop for FlushDenormals {
    #[inline]
    fn drop(&mut self) {
        write_mxcsr(self.previous_csr);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
fn read_mxcsr() -> u32 {
    let mut csr: u32 = 0;
    // SAFETY: `stmxcsr` only stores the SSE control register into the provided
    // 4-byte slot. SSE is part of the x86_64 / x86 baseline nih-plug targets.
    unsafe {
        core::arch::asm!(
            "stmxcsr [{ptr}]",
            ptr = in(reg) &mut csr,
            options(nostack, preserves_flags),
        );
    }
    csr
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
fn write_mxcsr(csr: u32) {
    // SAFETY: `ldmxcsr` only loads the SSE control register from the provided
    // 4-byte slot; the value is a previously read MXCSR with two bits set.
    unsafe {
        core::arch::asm!(
            "ldmxcsr [{ptr}]",
            ptr = in(reg) &csr,
            options(nostack, preserves_flags, readonly),
        );
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub struct FlushDenormals;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
impl FlushDenormals {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for FlushDenormals {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
