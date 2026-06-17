//! Cortex-M exception stack-frame types for QXK context switching.

/// Hardware-stacked registers (8 words) saved automatically by the processor
/// on any exception entry (Cortex-M3/M4/M7, no FPU lazy stacking).
///
/// The processor pushes these in descending-stack order: `r0` is at the lowest
/// address; `xpsr` is at the highest.  Aligning the total to 8 bytes is
/// enforced by the `STKALIGN` bit in `CCR` (set by default on M3/M4/M7).
///
/// For FPU-enabled builds (`thumbv7em-none-eabihf`) the hardware additionally
/// pushes `s0–s15` and `fpscr` when lazy stacking is active, but that is not
/// modelled here — the initialisation stub handles it via a separate
/// extended frame.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ContextFrame {
    /// Argument / result register.
    pub r0: u32,
    /// Argument / result register.
    pub r1: u32,
    /// Argument / result register.
    pub r2: u32,
    /// Argument / scratch register.
    pub r3: u32,
    /// Intra-procedure-call scratch register.
    pub r12: u32,
    /// Link register (return address from the calling function).
    pub lr: u32,
    /// Program counter (address of the next instruction to execute).
    pub pc: u32,
    /// Program status register (must have bit 24 set for Thumb mode).
    pub xpsr: u32,
}

impl ContextFrame {
    /// Creates a frame that will start execution at `entry` with argument `arg`.
    ///
    /// `lr` is set to a fault address (0xFFFF_FFFF) so that a thread returning
    /// from its entry function triggers a hard-fault rather than jumping to
    /// garbage — the thread must call `ThreadAction::Terminated` before it
    /// returns.
    pub const fn new(entry: u32, arg: u32) -> Self {
        Self {
            r0: arg,
            r1: 0,
            r2: 0,
            r3: 0,
            r12: 0,
            lr: 0xFFFF_FFFF, // fault sentinel — threads must not return
            pc: entry,
            xpsr: 0x0100_0000, // Thumb bit set, no exception active
        }
    }
}

/// Helper to initialise a raw byte buffer as a Cortex-M initial thread stack.
///
/// Writes a [`ContextFrame`] at the **top** of `storage` (highest address,
/// aligned down to 8 bytes) and returns a `*mut u8` pointing to the bottom of
/// the software-saved area (i.e. `r4–r11` region, currently zeroed) that the
/// PendSV handler will restore on first switch-in.
///
/// ```text
///  ← high addr ──────────────────────────── low addr →
///  [  xpsr | pc | lr | r12 | r3 | r2 | r1 | r0  ]  ← ContextFrame (32 bytes)
///  [  r11  | r10| r9 | r8  | r7 | r6 | r5 | r4  ]  ← software-saved (32 B)
///  ^                                                 ^
///  initial PSP set here after LDMIA r0!,{r4-r11}    returned as `initial_sp`
/// ```
pub struct ThreadStack<'a> {
    storage: &'a mut [u8],
}

impl<'a> ThreadStack<'a> {
    pub fn new(storage: &'a mut [u8]) -> Self {
        Self { storage }
    }

    /// Initialises the stack and returns the initial Process Stack Pointer
    /// (PSP) value to store in the thread control block.
    ///
    /// On first context-switch-in the PendSV handler will:
    ///  1. Load `r4–r11` via `LDMIA initial_sp!, {r4-r11}`.
    ///  2. Set PSP to the resulting pointer (which now points at the
    ///     [`ContextFrame`]).
    ///  3. Return from exception, causing the processor to auto-pop the frame.
    ///
    /// # Safety
    ///
    /// The returned pointer is only valid for the lifetime of `storage`.
    /// It must be stored in the thread's TCB and used only by the PendSV
    /// handler once the thread is scheduled.
    pub unsafe fn init(&mut self, entry: u32, arg: u32) -> *mut u8 {
        let len = self.storage.len();

        // Align stack top down to 8 bytes.
        let top = self.storage.as_mut_ptr().add(len);
        let frame_start = (top as usize - core::mem::size_of::<ContextFrame>()) & !7;

        // Write initial context frame.
        let frame_ptr = frame_start as *mut ContextFrame;
        core::ptr::write(frame_ptr, ContextFrame::new(entry, arg));

        // Reserve space for r4–r11 (8 × 4 bytes = 32 bytes) and zero it.
        let sw_save_start = frame_start - 8 * 4;
        core::ptr::write_bytes(sw_save_start as *mut u8, 0, 8 * 4);

        // PSP value to store in TCB.
        sw_save_start as *mut u8
    }
}
