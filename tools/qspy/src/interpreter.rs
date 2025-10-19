use std::collections::HashMap;

use crate::QsFrame;
use qs::predefined;
use qs::records::{qep, qf::time_evt, sched};
use qs::{FMT_MEM, FMT_STR, FMT_U16, FMT_U32, FMT_U64, FMT_U8};

/// Translates QS frames into human readable messages while tracking runtime
/// dictionaries.
#[derive(Default)]
pub struct FrameInterpreter {
    dict: Dictionaries,
}

impl FrameInterpreter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn interpret(&mut self, frame: &QsFrame) -> Vec<String> {
        let mut lines = Vec::new();
        match frame.record_type {
            predefined::SIG_DICT => self.handle_sig_dict(&frame.payload, &mut lines),
            predefined::OBJ_DICT => self.handle_obj_dict(&frame.payload, &mut lines),
            predefined::FUN_DICT => self.handle_fun_dict(&frame.payload, &mut lines),
            predefined::USR_DICT => self.handle_usr_dict(&frame.payload, &mut lines),
            predefined::TARGET_INFO => self.handle_target_info(&frame.payload, &mut lines),
            qep::STATE_ENTRY => self.handle_state_entry(&frame.payload, &mut lines),
            qep::STATE_EXIT => self.handle_state_exit(&frame.payload, &mut lines),
            qep::STATE_INIT => self.handle_state_init(&frame.payload, &mut lines),
            qep::INIT_TRAN => self.handle_init_tran(&frame.payload, &mut lines),
            qep::INTERN_TRAN => self.handle_intern_tran(&frame.payload, &mut lines),
            qep::TRAN => self.handle_tran(&frame.payload, &mut lines),
            qep::IGNORED => self.handle_ignored(&frame.payload, &mut lines),
            qep::DISPATCH => self.handle_dispatch(&frame.payload, &mut lines),
            qep::UNHANDLED => self.handle_unhandled(&frame.payload, &mut lines),
            time_evt::ARM => self.handle_time_evt_arm(&frame.payload, &mut lines),
            time_evt::AUTO_DISARM => self.handle_time_evt_auto_disarm(&frame.payload, &mut lines),
            time_evt::DISARM_ATTEMPT => {
                self.handle_time_evt_disarm_attempt(&frame.payload, &mut lines)
            }
            time_evt::DISARM => self.handle_time_evt_disarm(&frame.payload, &mut lines),
            time_evt::POST => self.handle_time_evt_post(&frame.payload, &mut lines),
            sched::LOCK => self.handle_sched_lock(&frame.payload, &mut lines),
            sched::UNLOCK => self.handle_sched_unlock(&frame.payload, &mut lines),
            sched::NEXT => self.handle_sched_next(&frame.payload, &mut lines),
            sched::IDLE => self.handle_sched_idle(&frame.payload, &mut lines),
            record if record >= 128 => self.handle_user_record(record, &frame.payload, &mut lines),
            _ => {}
        }

        if lines.is_empty() {
            lines.push(self.fallback_line(frame));
        }

        lines
    }

    fn fallback_line(&self, frame: &QsFrame) -> String {
        format!(
            "rec 0x{record:02X} len={len} payload={payload}",
            record = frame.record_type,
            len = frame.payload.len(),
            payload = hex_bytes(&frame.payload)
        )
    }

    fn handle_sig_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(signal), Some(object), Some(name)) =
            (cur.read_u16(), cur.read_u64(), cur.read_c_string())
        {
            self.dict.signals.insert((signal, object), name.clone());
            lines.push(format!(
                "Sig-Dict {signal:08X},Obj={obj}->{name}",
                obj = format_addr(object)
            ));
        }
    }

    fn handle_obj_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(addr), Some(name)) = (cur.read_u64(), cur.read_c_string()) {
            self.dict.objects.insert(addr, name.clone());
            lines.push(format!("Obj-Dict {addr}->{name}", addr = format_addr(addr)));
        }
    }

    fn handle_fun_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(addr), Some(name)) = (cur.read_u64(), cur.read_c_string()) {
            self.dict.functions.insert(addr, name.clone());
            lines.push(format!("Fun-Dict {addr}->{name}", addr = format_addr(addr)));
        }
    }

    fn handle_usr_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(id), Some(name)) = (cur.read_u8(), cur.read_c_string()) {
            self.dict.users.insert(id, name.clone());
            lines.push(format!("Usr-Dict {id:08}->{name}"));
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
            cur.read_u8(),
            cur.read_u16(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
            cur.read_u8(),
        ) {
            let stamp = format!("{day:02}{month:02}{year:02}_{hour:02}{minute:02}{second:02}");
            let reset_tag = if reset == 0xFF { "RST" } else { "INF" };
            lines.push(format!("Trg-{reset_tag}  QP-Ver={version},Build={stamp}"));
            lines.push(format!(
                "           Cfg Sig/Evt={sig_evt:#04X} Eq/Te={eq_te:#04X} Mp={mp_sizes:#04X} Ptr={ptr_sizes:#04X} Time={time_size:#04X} Active={max_active} Pools/Ticks={max_pool_tick:#04X}"
            ));
        }
    }

    fn handle_state_entry(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(state)) = (cur.read_u64(), cur.read_u64()) {
            lines.push(format!(
                "===RTC===> St-Entry Obj={obj},State={state}",
                obj = self.dict.object_name(obj),
                state = self.dict.function_name(state)
            ));
        }
    }

    fn handle_state_exit(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(state)) = (cur.read_u64(), cur.read_u64()) {
            lines.push(format!(
                "===RTC===> St-Exit  Obj={obj},State={state}",
                obj = self.dict.object_name(obj),
                state = self.dict.function_name(state)
            ));
        }
    }

    fn handle_state_init(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(source), Some(target)) =
            (cur.read_u64(), cur.read_u64(), cur.read_u64())
        {
            lines.push(format!(
                "===RTC===> St-Init  Obj={obj},State={src}->{tgt}",
                obj = self.dict.object_name(obj),
                src = self.dict.function_name(source),
                tgt = self.dict.function_name(target)
            ));
        }
    }

    fn handle_init_tran(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(obj), Some(target)) =
            (cur.read_u32(), cur.read_u64(), cur.read_u64())
        {
            lines.push(format!(
                "{ts:010} Init===> Obj={obj},State={state}",
                obj = self.dict.object_name(obj),
                state = self.dict.function_name(target)
            ));
        }
    }

    fn handle_intern_tran(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(state)) = (
            cur.read_u32(),
            cur.read_u16(),
            cur.read_u64(),
            cur.read_u64(),
        ) {
            lines.push(format!(
                "{ts:010} =>Intern Obj={obj},Sig={sig},State={state}",
                obj = self.dict.object_name(obj),
                sig = self.dict.signal_name(signal, obj),
                state = self.dict.function_name(state)
            ));
        }
    }

    fn handle_tran(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(source), Some(target)) = (
            cur.read_u32(),
            cur.read_u16(),
            cur.read_u64(),
            cur.read_u64(),
            cur.read_u64(),
        ) {
            lines.push(format!(
                "{ts:010} ===>Tran Obj={obj},Sig={sig},State={src}->{tgt}",
                obj = self.dict.object_name(obj),
                sig = self.dict.signal_name(signal, obj),
                src = self.dict.function_name(source),
                tgt = self.dict.function_name(target)
            ));
        }
    }

    fn handle_ignored(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(state)) = (
            cur.read_u32(),
            cur.read_u16(),
            cur.read_u64(),
            cur.read_u64(),
        ) {
            lines.push(format!(
                "{ts:010} =>Ignored Obj={obj},Sig={sig},State={state}",
                obj = self.dict.object_name(obj),
                sig = self.dict.signal_name(signal, obj),
                state = self.dict.function_name(state)
            ));
        }
    }

    fn handle_unhandled(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(signal), Some(obj), Some(state)) =
            (cur.read_u16(), cur.read_u64(), cur.read_u64())
        {
            lines.push(format!(
                "=>Unhandled Obj={obj},Sig={sig},State={state}",
                obj = self.dict.object_name(obj),
                sig = self.dict.signal_name(signal, obj),
                state = self.dict.function_name(state)
            ));
        }
    }

    fn handle_dispatch(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(signal), Some(obj), Some(state)) = (
            cur.read_u32(),
            cur.read_u16(),
            cur.read_u64(),
            cur.read_u64(),
        ) {
            lines.push(format!(
                "{ts:010} Disp===> Obj={obj},Sig={sig},State={state}",
                obj = self.dict.object_name(obj),
                sig = self.dict.signal_name(signal, obj),
                state = self.dict.function_name(state)
            ));
        }
    }

    fn handle_time_evt_arm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(target), Some(timeout), Some(interval), Some(rate)) = (
            cur.read_u32(),
            cur.read_u64(),
            cur.read_u64(),
            cur.read_u16(),
            cur.read_u16(),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-Arm  Obj={timer},AO={target},Tim={timeout},Int={interval}",
                timer = self.dict.object_name(timer),
                target = self.dict.object_name(target)
            ));
        }
    }

    fn handle_time_evt_auto_disarm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(timer), Some(target), Some(rate)) =
            (cur.read_u64(), cur.read_u64(), cur.read_u8())
        {
            lines.push(format!(
                "           TE{rate}-ADis Obj={timer},AO={target}",
                timer = self.dict.object_name(timer),
                target = self.dict.object_name(target)
            ));
        }
    }

    fn handle_time_evt_disarm_attempt(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(target), Some(rate)) = (
            cur.read_u32(),
            cur.read_u64(),
            cur.read_u64(),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-DisA Obj={timer},AO={target}",
                timer = self.dict.object_name(timer),
                target = self.dict.object_name(target)
            ));
        }
    }

    fn handle_time_evt_disarm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(target), Some(remaining), Some(interval), Some(rate)) = (
            cur.read_u32(),
            cur.read_u64(),
            cur.read_u64(),
            cur.read_u16(),
            cur.read_u16(),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-DisA Obj={timer},AO={target},Tim={remaining},Int={interval}",
                timer = self.dict.object_name(timer),
                target = self.dict.object_name(target)
            ));
        }
    }

    fn handle_time_evt_post(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(timer), Some(signal), Some(target), Some(rate)) = (
            cur.read_u32(),
            cur.read_u64(),
            cur.read_u16(),
            cur.read_u64(),
            cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} TE{rate}-Post Obj={timer},Sig={sig},AO={target}",
                timer = self.dict.object_name(timer),
                sig = self.dict.signal_name(signal, target),
                target = self.dict.object_name(target)
            ));
        }
    }

    fn handle_sched_lock(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev), Some(new)) = (cur.read_u32(), cur.read_u8(), cur.read_u8()) {
            lines.push(format!("{ts:010} Sch-Lock Prev={prev} New={new}"));
        }
    }

    fn handle_sched_unlock(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev), Some(new)) = (cur.read_u32(), cur.read_u8(), cur.read_u8()) {
            lines.push(format!("{ts:010} Sch-Unlk Prev={prev} New={new}"));
        }
    }

    fn handle_sched_next(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(cur_prio), Some(prev_prio)) =
            (cur.read_u32(), cur.read_u8(), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Next Pri={prev_prio}->{cur_prio}"));
        }
    }

    fn handle_sched_idle(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev)) = (cur.read_u32(), cur.read_u8()) {
            lines.push(format!("{ts:010} Sch-Idle Pri={prev}->0"));
        }
    }

    fn handle_user_record(&mut self, record: u8, payload: &[u8], lines: &mut Vec<String>) {
        let name = self
            .dict
            .users
            .get(&record)
            .cloned()
            .unwrap_or_else(|| format!("USR({record})"));

        if payload.len() < 4 {
            lines.push(format!("{name} payload={}", hex_bytes(payload)));
            return;
        }

        let ts = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let mut cur = Cursor::new(&payload[4..]);
        let mut values = Vec::new();
        while let Some(format) = cur.read_u8() {
            let base = format & 0x0F;
            match base {
                FMT_U8 => {
                    if let Some(value) = cur.read_u8() {
                        values.push(value.to_string());
                    } else {
                        break;
                    }
                }
                FMT_U16 => {
                    if let Some(value) = cur.read_u16() {
                        values.push(value.to_string());
                    } else {
                        break;
                    }
                }
                FMT_U32 => {
                    if let Some(value) = cur.read_u32() {
                        values.push(value.to_string());
                    } else {
                        break;
                    }
                }
                FMT_U64 => {
                    if let Some(value) = cur.read_u64() {
                        values.push(format_addr(value));
                    } else {
                        break;
                    }
                }
                FMT_STR => {
                    if let Some(value) = cur.read_c_string() {
                        values.push(value);
                    } else {
                        break;
                    }
                }
                FMT_MEM => {
                    if let Some(len) = cur.read_u8() {
                        let blob = cur.read_bytes(len as usize);
                        match blob {
                            Some(data) => values.push(format!("mem:{}", hex_bytes(data))),
                            None => break,
                        }
                    } else {
                        break;
                    }
                }
                _ => {
                    values.push(format!("fmt0x{format:02X}"));
                    break;
                }
            }
        }
        lines.push(format!("{ts:010} {name} {}", values.join(" ")));
    }
}

#[derive(Default)]
struct Dictionaries {
    objects: HashMap<u64, String>,
    functions: HashMap<u64, String>,
    signals: HashMap<(u16, u64), String>,
    users: HashMap<u8, String>,
}

impl Dictionaries {
    fn object_name(&self, addr: u64) -> String {
        self.objects
            .get(&addr)
            .cloned()
            .unwrap_or_else(|| format_addr(addr))
    }

    fn function_name(&self, addr: u64) -> String {
        self.functions
            .get(&addr)
            .cloned()
            .unwrap_or_else(|| format_addr(addr))
    }

    fn signal_name(&self, signal: u16, obj: u64) -> String {
        if let Some(name) = self.signals.get(&(signal, obj)) {
            return name.clone();
        }

        if let Some(name) = self.signals.get(&(signal, 0)) {
            return name.clone();
        }

        format!("Signal({signal})")
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.pos + count > self.data.len() {
            None
        } else {
            let slice = &self.data[self.pos..self.pos + count];
            self.pos += count;
            Some(slice)
        }
    }

    fn read_u8(&mut self) -> Option<u8> {
        self.read_bytes(1).map(|b| b[0])
    }

    fn read_u16(&mut self) -> Option<u16> {
        self.read_bytes(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> Option<u32> {
        self.read_bytes(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64(&mut self) -> Option<u64> {
        self.read_bytes(8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn read_c_string(&mut self) -> Option<String> {
        let remaining = &self.data[self.pos..];
        let end = remaining.iter().position(|&b| b == 0)?;
        let bytes = &remaining[..end];
        self.pos += end + 1;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

fn format_addr(addr: u64) -> String {
    format!("0x{addr:016X}")
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02X}");
    }
    out
}
