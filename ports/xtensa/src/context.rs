//! Xtensa exception stack-frame types for QXK context switching.

/// Layout of registers saved during an exception/trap on Xtensa LX.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ContextFrame {
    pub pc: u32,
    pub ps: u32,
    pub a0: u32,
    pub a1: u32, // Stack pointer
    pub a2: u32,
    pub a3: u32,
    pub a4: u32,
    pub a5: u32,
    pub a6: u32,
    pub a7: u32,
    pub a8: u32,
    pub a9: u32,
    pub a10: u32,
    pub a11: u32,
    pub a12: u32,
    pub a13: u32,
    pub a14: u32,
    pub a15: u32,
    pub sar: u32,
}

impl ContextFrame {
    /// Creates a frame that will start execution at `entry` with argument `arg`.
    pub const fn new(entry: u32, arg: u32) -> Self {
        Self {
            pc: entry,
            ps: 0x00040000, // WOE set (Window Overflow Enable), INTLEVEL = 0
            a0: 0,          // return address (fault sentinel)
            a1: 0,          // stack pointer (filled in by ThreadStack::init)
            a2: arg,        // first argument
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
            a13: 0,
            a14: 0,
            a15: 0,
            sar: 0,
        }
    }
}

/// Helper to initialise a raw byte buffer as an Xtensa initial thread stack.
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
        let mut frame = ContextFrame::new(entry, arg);
        frame.a1 = frame_start as u32;
        core::ptr::write(frame_ptr, frame);

        frame_start as *mut u8
    }
}
