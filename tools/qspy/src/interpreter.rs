use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use crate::cursor::Cursor;
use crate::sizes::TargetSizes;
use crate::QsFrame;
use qs::predefined;
use qs::records::{qep, qf::time_evt, sched};
use qs::{
    FMT_F32, FMT_F64, FMT_FUN, FMT_HEX, FMT_I16, FMT_I32, FMT_I64, FMT_I8_ENUM, FMT_MEM,
    FMT_OBJ, FMT_SIG, FMT_STR, FMT_U16, FMT_U32, FMT_U64, FMT_U8,
};

/// Translates QS frames into human-readable messages while tracking runtime dictionaries.
pub struct FrameInterpreter {
    dict:  Dictionaries,
    sizes: TargetSizes,
}

impl Default for FrameInterpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameInterpreter {
    pub fn new() -> Self {
        Self {
            dict:  Dictionaries::default(),
            sizes: TargetSizes::default(),
        }
    }

    pub fn with_sizes(sizes: TargetSizes) -> Self {
        Self { dict: Dictionaries::default(), sizes }
    }

    pub fn sizes(&self) -> &TargetSizes {
        &self.sizes
    }

    pub fn set_sizes(&mut self, s: TargetSizes) {
        self.sizes = s;
    }

    pub fn interpret(&mut self, frame: &QsFrame) -> Vec<String> {
        let mut lines = Vec::new();
        match frame.record_type {
            predefined::SIG_DICT    => self.handle_sig_dict(&frame.payload, &mut lines),
            predefined::OBJ_DICT    => self.handle_obj_dict(&frame.payload, &mut lines),
            predefined::FUN_DICT    => self.handle_fun_dict(&frame.payload, &mut lines),
            predefined::USR_DICT    => self.handle_usr_dict(&frame.payload, &mut lines),
            predefined::TARGET_INFO => self.handle_target_info(&frame.payload, &mut lines),
            qep::STATE_ENTRY    => self.handle_state_entry(&frame.payload, &mut lines),
            qep::STATE_EXIT     => self.handle_state_exit(&frame.payload, &mut lines),
            qep::STATE_INIT     => self.handle_state_init(&frame.payload, &mut lines),
            qep::INIT_TRAN      => self.handle_init_tran(&frame.payload, &mut lines),
            qep::INTERN_TRAN    => self.handle_intern_tran(&frame.payload, &mut lines),
            qep::TRAN           => self.handle_tran(&frame.payload, &mut lines),
            qep::IGNORED        => self.handle_ignored(&frame.payload, &mut lines),
            qep::DISPATCH       => self.handle_dispatch(&frame.payload, &mut lines),
            qep::UNHANDLED      => self.handle_unhandled(&frame.payload, &mut lines),
            time_evt::ARM            => self.handle_time_evt_arm(&frame.payload, &mut lines),
            time_evt::AUTO_DISARM    => self.handle_time_evt_auto_disarm(&frame.payload, &mut lines),
            time_evt::DISARM_ATTEMPT => self.handle_time_evt_disarm_attempt(&frame.payload, &mut lines),
            time_evt::DISARM         => self.handle_time_evt_disarm(&frame.payload, &mut lines),
            time_evt::POST           => self.handle_time_evt_post(&frame.payload, &mut lines),
            sched::LOCK   => self.handle_sched_lock(&frame.payload, &mut lines),
            sched::UNLOCK => self.handle_sched_unlock(&frame.payload, &mut lines),
            sched::NEXT   => self.handle_sched_next(&frame.payload, &mut lines),
            sched::IDLE   => self.handle_sched_idle(&frame.payload, &mut lines),
            65 => lines.push(format!("Trg-Done rec={}", frame.payload.first().copied().unwrap_or(0))),
            66 => self.handle_rx_status(&frame.payload, &mut lines),
            rec if rec >= 128 => self.handle_user_record(rec, &frame.payload, &mut lines),
            _ => {}
        }

        if lines.is_empty() {
            lines.push(self.fallback_line(frame));
        }
        lines
    }

    // ── Dictionary helpers ────────────────────────────────────────────────────

    fn obj_str(&self, addr: u64) -> String {
        self.dict.objects.get(&addr)
            .cloned()
            .unwrap_or_else(|| TargetSizes::fmt_addr(addr, self.sizes.obj_ptr_size))
    }

    fn fun_str(&self, addr: u64) -> String {
        self.dict.functions.get(&addr)
            .cloned()
            .unwrap_or_else(|| TargetSizes::fmt_addr(addr, self.sizes.fun_ptr_size))
    }

    fn sig_str(&self, signal: u64, obj: u64) -> String {
        let sig16 = signal as u16;
        if let Some(name) = self.dict.signals.get(&(sig16, obj)).or_else(|| self.dict.signals.get(&(sig16, 0))) {
            return name.clone();
        }
        format!("Sig({signal})")
    }

    // ── Predefined record handlers ────────────────────────────────────────────

    fn handle_sig_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(signal), Some(object), Some(name)) = (
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_c_string(),
        ) {
            self.dict.signals.insert((signal as u16, object), name.clone());
            lines.push(format!(
                "Sig-Dict {signal:#010X},Obj={obj}->{name}",
                obj = TargetSizes::fmt_addr(object, self.sizes.obj_ptr_size)
            ));
        }
    }

    fn handle_obj_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(addr), Some(name)) =
            (cur.read_sized(self.sizes.obj_ptr_size), cur.read_c_string())
        {
            self.dict.objects.insert(addr, name.clone());
            lines.push(format!(
                "Obj-Dict {addr}->{name}",
                addr = TargetSizes::fmt_addr(addr, self.sizes.obj_ptr_size)
            ));
        }
    }

    fn handle_fun_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(addr), Some(name)) =
            (cur.read_sized(self.sizes.fun_ptr_size), cur.read_c_string())
        {
            self.dict.functions.insert(addr, name.clone());
            lines.push(format!(
                "Fun-Dict {addr}->{name}",
                addr = TargetSizes::fmt_addr(addr, self.sizes.fun_ptr_size)
            ));
        }
    }

    fn handle_usr_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(id), Some(name)) = (cur.read_u8(), cur.read_c_string()) {
            self.dict.users.insert(id, name.clone());
            lines.push(format!("Usr-Dict {id:03}->{name}"));
        }
    }

    fn handle_target_info(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (
            Some(reset),
            Some(version),
            Some(sig_evt),
            Some(eq_te),
            Some(mp_sizes),
            Some(ptr_sizes),
            Some(time_size),
            Some(max_active),
            Some(max_pool_tick),
            Some(second),
            Some(minute),
            Some(hour),
            Some(day),
            Some(month),
            Some(year),
        ) = (
            cur.read_u8(),  cur.read_u16(), cur.read_u8(), cur.read_u8(),
            cur.read_u8(),  cur.read_u8(),  cur.read_u8(), cur.read_u8(),
            cur.read_u8(),  cur.read_u8(),  cur.read_u8(), cur.read_u8(),
            cur.read_u8(),  cur.read_u8(),  cur.read_u8(),
        ) {
            let stamp = format!("{day:02}{month:02}{year:02}_{hour:02}{minute:02}{second:02}");
            let reset_tag = if reset == 0xFF { "RST" } else { "INF" };
            lines.push(format!("Trg-{reset_tag}  QP-Ver={version},Build={stamp}"));
            lines.push(format!(
                "           Cfg Sig/Evt={sig_evt:#04X} Eq/Te={eq_te:#04X} Mp={mp_sizes:#04X} \
                 Ptr={ptr_sizes:#04X} Time={time_size:#04X} Active={max_active} \
                 Pools/Ticks={max_pool_tick:#04X}"
            ));

            // Update runtime sizes from target report.
            self.sizes.update_from_target_info(payload);
        }
    }

    fn handle_rx_status(&self, payload: &[u8], lines: &mut Vec<String>) {
        if let Some(&b) = payload.first() {
            if b & 0x80 != 0 {
                lines.push(format!("QS-RX Err={:#04X}", b & 0x7F));
            } else {
                lines.push(format!("QS-RX Ack rec={b}"));
            }
        }
    }

    fn handle_state_entry(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(state)) = (
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "===RTC===> St-Entry Obj={},State={}",
                self.obj_str(obj), self.fun_str(state)
            ));
        }
    }

    fn handle_state_exit(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(state)) = (
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "===RTC===> St-Exit  Obj={},State={}",
                self.obj_str(obj), self.fun_str(state)
            ));
        }
    }

    fn handle_state_init(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(src), Some(tgt)) = (
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "===RTC===> St-Init  Obj={},State={}->{}",
                self.obj_str(obj), self.fun_str(src), self.fun_str(tgt)
            ));
        }
    }

    fn handle_init_tran(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(obj), Some(tgt)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} Init===> Obj={},State={}",
                self.obj_str(obj), self.fun_str(tgt)
            ));
        }
    }

    fn handle_intern_tran(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(state)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} =>Intern Obj={},Sig={},State={}",
                self.obj_str(obj), self.sig_str(signal, obj), self.fun_str(state)
            ));
        }
    }

    fn handle_tran(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(src), Some(tgt)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} ===>Tran Obj={},Sig={},State={}->{}",
                self.obj_str(obj), self.sig_str(signal, obj),
                self.fun_str(src), self.fun_str(tgt)
            ));
        }
    }

    fn handle_ignored(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(state)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} =>Ignored Obj={},Sig={},State={}",
                self.obj_str(obj), self.sig_str(signal, obj), self.fun_str(state)
            ));
        }
    }

    fn handle_dispatch(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(state)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} Disp===> Obj={},Sig={},State={}",
                self.obj_str(obj), self.sig_str(signal, obj), self.fun_str(state)
            ));
        }
    }

    fn handle_unhandled(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(signal), Some(obj), Some(state)) = (
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "=>Unhandled Obj={},Sig={},State={}",
                self.obj_str(obj), self.sig_str(signal, obj), self.fun_str(state)
            ));
        }
    }

    fn handle_time_evt_arm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(target), Some(timeout), Some(interval), Some(rate)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.timeevt_ctr),
            cur.read_sized(self.sizes.timeevt_ctr),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-Arm  Obj={},AO={},Tim={timeout},Int={interval}",
                self.obj_str(timer), self.obj_str(target)
            ));
        }
    }

    fn handle_time_evt_auto_disarm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(timer), Some(target), Some(rate)) = (
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "           TE{rate}-ADis Obj={},AO={}",
                self.obj_str(timer), self.obj_str(target)
            ));
        }
    }

    fn handle_time_evt_disarm_attempt(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(target), Some(rate)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-DisA Obj={},AO={}",
                self.obj_str(timer), self.obj_str(target)
            ));
        }
    }

    fn handle_time_evt_disarm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(target), Some(remaining), Some(interval), Some(rate)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.timeevt_ctr),
            cur.read_sized(self.sizes.timeevt_ctr),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-Dis  Obj={},AO={},Tim={remaining},Int={interval}",
                self.obj_str(timer), self.obj_str(target)
            ));
        }
    }

    fn handle_time_evt_post(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(signal), Some(target), Some(rate)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-Post Obj={},Sig={},AO={}",
                self.obj_str(timer),
                self.sig_str(signal, target),
                self.obj_str(target)
            ));
        }
    }

    fn handle_sched_lock(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev), Some(new)) =
            (cur.read_sized(self.sizes.time_size), cur.read_u8(), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Lock Prev={prev} New={new}"));
        }
    }

    fn handle_sched_unlock(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev), Some(new)) =
            (cur.read_sized(self.sizes.time_size), cur.read_u8(), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Unlk Prev={prev} New={new}"));
        }
    }

    fn handle_sched_next(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(cur_prio), Some(prev_prio)) =
            (cur.read_sized(self.sizes.time_size), cur.read_u8(), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Next Pri={prev_prio}->{cur_prio}"));
        }
    }

    fn handle_sched_idle(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev)) =
            (cur.read_sized(self.sizes.time_size), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Idle Pri={prev}->0"));
        }
    }

    fn handle_user_record(&mut self, record: u8, payload: &[u8], lines: &mut Vec<String>) {
        let name = self.dict.users.get(&record).cloned()
            .unwrap_or_else(|| format!("USR({record})"));

        let mut cur = Cursor::new(payload);
        let ts = match cur.read_sized(self.sizes.time_size) {
            Some(v) => v,
            None => {
                lines.push(format!("{name} payload={}", hex_bytes(payload)));
                return;
            }
        };

        let mut values: Vec<String> = Vec::new();
        let mut hex_flag = false;

        while let Some(fmt_byte) = cur.read_u8() {
            let base = fmt_byte & 0x0F;
            match base {
                FMT_I8_ENUM => {
                    if let Some(v) = cur.read_u8() {
                        let s = (v as i8).to_string();
                        values.push(if hex_flag { format!("{v:#04X}") } else { s });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_U8 => {
                    if let Some(v) = cur.read_u8() {
                        values.push(if hex_flag { format!("{v:#04X}") } else { v.to_string() });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_I16 => {
                    if let Some(v) = cur.read_u16() {
                        let s = (v as i16).to_string();
                        values.push(if hex_flag { format!("{v:#06X}") } else { s });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_U16 => {
                    if let Some(v) = cur.read_u16() {
                        values.push(if hex_flag { format!("{v:#06X}") } else { v.to_string() });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_I32 => {
                    if let Some(v) = cur.read_u32() {
                        let s = (v as i32).to_string();
                        values.push(if hex_flag { format!("{v:#010X}") } else { s });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_U32 => {
                    if let Some(v) = cur.read_u32() {
                        values.push(if hex_flag { format!("{v:#010X}") } else { v.to_string() });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_F32 => {
                    if let Some(bytes) = cur.read_bytes(4) {
                        let v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                        values.push(format!("{v:.6}"));
                    } else { break; }
                    hex_flag = false;
                }
                FMT_F64 => {
                    if let Some(bytes) = cur.read_bytes(8) {
                        let v = f64::from_le_bytes([
                            bytes[0], bytes[1], bytes[2], bytes[3],
                            bytes[4], bytes[5], bytes[6], bytes[7],
                        ]);
                        values.push(format!("{v:.6}"));
                    } else { break; }
                    hex_flag = false;
                }
                FMT_STR => {
                    if let Some(s) = cur.read_c_string() { values.push(s); } else { break; }
                    hex_flag = false;
                }
                FMT_MEM => {
                    if let Some(len) = cur.read_u8() {
                        match cur.read_bytes(len as usize) {
                            Some(data) => values.push(format!("mem:{}", hex_bytes(data))),
                            None => break,
                        }
                    } else { break; }
                    hex_flag = false;
                }
                FMT_SIG => {
                    if let Some(v) = cur.read_sized(self.sizes.signal_size) {
                        values.push(self.sig_str(v, 0));
                    } else { break; }
                    hex_flag = false;
                }
                FMT_OBJ => {
                    if let Some(v) = cur.read_sized(self.sizes.obj_ptr_size) {
                        values.push(self.obj_str(v));
                    } else { break; }
                    hex_flag = false;
                }
                FMT_FUN => {
                    if let Some(v) = cur.read_sized(self.sizes.fun_ptr_size) {
                        values.push(self.fun_str(v));
                    } else { break; }
                    hex_flag = false;
                }
                FMT_I64 => {
                    if let Some(v) = cur.read_u64() {
                        let s = (v as i64).to_string();
                        values.push(if hex_flag { format!("{v:#018X}") } else { s });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_U64 => {
                    if let Some(v) = cur.read_u64() {
                        values.push(if hex_flag { format!("{v:#018X}") } else { v.to_string() });
                    } else { break; }
                    hex_flag = false;
                }
                FMT_HEX => {
                    hex_flag = true;
                    continue;
                }
                _ => {
                    values.push(format!("fmt={fmt_byte:#04X}"));
                    break;
                }
            }
        }

        lines.push(format!("{ts:010} {name} {}", values.join(" ")));
    }

    fn fallback_line(&self, frame: &QsFrame) -> String {
        format!(
            "rec={:#04X} len={} payload={}",
            frame.record_type,
            frame.payload.len(),
            hex_bytes(&frame.payload)
        )
    }

    // ── Dictionary persistence ────────────────────────────────────────────────

    /// Save the accumulated dictionaries to a plain-text file.
    pub fn save_dictionaries(&self, path: &Path) -> io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut w = io::BufWriter::new(file);
        writeln!(w, "# qspy dictionary")?;
        for (addr, name) in &self.dict.objects {
            writeln!(w, "OBJ 0x{addr:016X} {name}")?;
        }
        for (addr, name) in &self.dict.functions {
            writeln!(w, "FUN 0x{addr:016X} {name}")?;
        }
        for ((sig, obj), name) in &self.dict.signals {
            writeln!(w, "SIG {sig} 0x{obj:016X} {name}")?;
        }
        for (id, name) in &self.dict.users {
            writeln!(w, "USR {id} {name}")?;
        }
        Ok(())
    }

    /// Load dictionaries from a file previously saved by `save_dictionaries`.
    pub fn load_dictionaries(&mut self, path: &Path) -> io::Result<()> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            match parts.as_slice() {
                ["OBJ", addr, name] => {
                    if let Ok(a) = parse_addr(addr) {
                        self.dict.objects.insert(a, (*name).to_owned());
                    }
                }
                ["FUN", addr, name] => {
                    if let Ok(a) = parse_addr(addr) {
                        self.dict.functions.insert(a, (*name).to_owned());
                    }
                }
                ["SIG", sig, obj, name] => {
                    if let (Ok(s), Ok(o)) = (sig.parse::<u16>(), parse_addr(obj)) {
                        self.dict.signals.insert((s, o), (*name).to_owned());
                    }
                }
                ["USR", id, name] => {
                    if let Ok(i) = id.parse::<u8>() {
                        self.dict.users.insert(i, (*name).to_owned());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ── Dictionaries ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct Dictionaries {
    objects:   HashMap<u64, String>,
    functions: HashMap<u64, String>,
    signals:   HashMap<(u16, u64), String>,
    users:     HashMap<u8, String>,
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02X}");
    }
    out
}

fn parse_addr(s: &str) -> Result<u64, std::num::ParseIntError> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(hex, 16)
}
