//! RISC-V exception stack-frame types for QXK context switching.

/// Layout of registers saved during an exception/trap on RISC-V.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ContextFrame {
    pub ra: u32,
    pub t0: u32,
    pub t1: u32,
    pub t2: u32,
    pub a0: u32,
    pub a1: u32,
    pub a2: u32,
    pub a3: u32,
    pub a4: u32,
    pub a5: u32,
    pub a6: u32,
    pub a7: u32,
    pub t3: u32,
    pub t4: u32,
    pub t5: u32,
    pub t6: u32,
    pub mepc: u32,
    pub mstatus: u32,
}

impl ContextFrame {
    /// Creates a frame that will start execution at `entry` with argument `arg`.
    pub const fn new(entry: u32, arg: u32) -> Self {
        Self {
            ra: 0xFFFF_FFFF, // return address (fault sentinel)
            t0: 0,
            t1: 0,
            t2: 0,
            a0: arg, // first argument
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            t3: 0,
            t4: 0,
            t5: 0,
            t6: 0,
            mepc: entry,
            mstatus: 0x1880, // MPP = 11 (Machine mode), MPIE = 1 (Enable interrupts on mret)
        }
    }
}

/// Helper to initialise a raw byte buffer as a RISC-V initial thread stack.
pub struct ThreadStack<'a> {
    storage: &'a mut [u8],
}

impl<'a> ThreadStack<'a> {
    pub fn new(storage: &'a mut [u8]) -> Self {
        Self { storage }
    }

    /// Initialises the stack and returns the initial process stack pointer.
    ///
    /// # Safety
    /// The returned pointer is only valid for the lifetime of `storage`.
    pub unsafe fn init(&mut self, entry: u32, arg: u32) -> *mut u8 {
        let len = self.storage.len();

        // Align stack top down to 16 bytes.
        let top = self.storage.as_mut_ptr().add(len);
        let frame_start = (top as usize - core::mem::size_of::<ContextFrame>()) & !15;

        // Write initial context frame.
        let frame_ptr = frame_start as *mut ContextFrame;
        core::ptr::write(frame_ptr, ContextFrame::new(entry, arg));

        // Reserve space for s0–s11 (12 × 4 bytes = 48 bytes) and zero it.
        let sw_save_start = frame_start - 12 * 4;
        core::ptr::write_bytes(sw_save_start as *mut u8, 0, 12 * 4);

        sw_save_start as *mut u8
    }
}
