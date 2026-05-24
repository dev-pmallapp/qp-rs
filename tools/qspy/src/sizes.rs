/// Target-side type widths, reported via `TARGET_INFO` and overridable via CLI flags.
///
/// All sizes are in bytes. Valid values are 1, 2, 4, or 8 (invalid packed nibbles fall
/// back to the field's current default).
#[derive(Debug, Clone, Copy)]
pub struct TargetSizes {
    pub time_size:    u8,
    pub obj_ptr_size: u8,
    pub fun_ptr_size: u8,
    pub signal_size:  u8,
    pub event_size:   u8,
    pub equeue_ctr:   u8,
    pub timeevt_ctr:  u8,
    pub mpool_siz:    u8,
    pub mpool_ctr:    u8,
}

impl Default for TargetSizes {
    fn default() -> Self {
        Self {
            time_size:    4,
            obj_ptr_size: 4,
            fun_ptr_size: 4,
            signal_size:  2,
            event_size:   2,
            equeue_ctr:   1,
            timeevt_ctr:  2,
            mpool_siz:    2,
            mpool_ctr:    2,
        }
    }
}

impl TargetSizes {
    /// Update from a `TARGET_INFO` frame payload (starting at `payload[0]` = `is_reset`).
    ///
    /// The packed byte layout matches `predefined::target_info_payload`:
    /// - `payload[3]`: `signal_size | (event_size << 4)`
    /// - `payload[4]`: `equeue_ctr | (timeevt_ctr << 4)`
    /// - `payload[5]`: `mpool_siz  | (mpool_ctr   << 4)`
    /// - `payload[6]`: `obj_ptr    | (fun_ptr      << 4)`
    /// - `payload[7]`: `time_size`
    pub fn update_from_target_info(&mut self, payload: &[u8]) {
        if payload.len() < 8 {
            return;
        }
        let sig_evt = payload[3];
        let eq_te   = payload[4];
        let mp      = payload[5];
        let ptrs    = payload[6];
        let time    = payload[7];

        self.signal_size  = valid_size(sig_evt & 0x0F, self.signal_size);
        self.event_size   = valid_size((sig_evt >> 4) & 0x0F, self.event_size);
        self.equeue_ctr   = valid_size(eq_te & 0x0F, self.equeue_ctr);
        self.timeevt_ctr  = valid_size((eq_te >> 4) & 0x0F, self.timeevt_ctr);
        self.mpool_siz    = valid_size(mp & 0x0F, self.mpool_siz);
        self.mpool_ctr    = valid_size((mp >> 4) & 0x0F, self.mpool_ctr);
        self.obj_ptr_size = valid_size(ptrs & 0x0F, self.obj_ptr_size);
        self.fun_ptr_size = valid_size((ptrs >> 4) & 0x0F, self.fun_ptr_size);
        self.time_size    = valid_size(time, self.time_size);
    }

    /// Format an address with the correct hex width for the given pointer size.
    pub fn fmt_addr(addr: u64, size: u8) -> String {
        match size {
            1 => format!("0x{addr:02X}"),
            2 => format!("0x{addr:04X}"),
            4 => format!("0x{addr:08X}"),
            _ => format!("0x{addr:016X}"),
        }
    }
}

fn valid_size(v: u8, fallback: u8) -> u8 {
    match v {
        1 | 2 | 4 | 8 => v,
        _ => fallback,
    }
}
