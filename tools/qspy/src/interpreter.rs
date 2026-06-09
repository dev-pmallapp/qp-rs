use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use crate::cursor::Cursor;
use crate::sizes::TargetSizes;
use crate::QsFrame;
use qs::predefined;
use qs::records::{infra, qep, qf, qf::time_evt, sched};
use qs::{
    FMT_F32, FMT_F64, FMT_FUN, FMT_HEX, FMT_I16, FMT_I32, FMT_I64, FMT_I8_ENUM, FMT_MEM,
    FMT_OBJ, FMT_SIG, FMT_STR, FMT_U16, FMT_U32, FMT_U64, FMT_U8,
};

/// Translates QS frames into human-readable messages while tracking runtime dictionaries.
pub struct FrameInterpreter {
    dict:       Dictionaries,
    sizes:      TargetSizes,
    qs_version: u16,
}

impl Default for FrameInterpreter {
    fn default() -> Self { Self::new() }
}

impl FrameInterpreter {
    pub fn new() -> Self {
        Self { dict: Dictionaries::default(), sizes: TargetSizes::default(), qs_version: 700 }
    }

    pub fn with_sizes(sizes: TargetSizes) -> Self {
        Self { dict: Dictionaries::default(), sizes, qs_version: 700 }
    }

    pub fn sizes(&self) -> &TargetSizes { &self.sizes }
    pub fn set_sizes(&mut self, s: TargetSizes) { self.sizes = s; }
    pub fn set_qs_version(&mut self, v: u16) { self.qs_version = v; }

    pub fn interpret(&mut self, frame: &QsFrame) -> Vec<String> {
        let mut lines = Vec::new();
        match frame.record_type {
            // ── Dictionaries & target info ─────────────────────────────────
            predefined::ENUM_DICT   => self.handle_enum_dict(&frame.payload, &mut lines),
            predefined::SIG_DICT    => self.handle_sig_dict(&frame.payload, &mut lines),
            predefined::OBJ_DICT    => self.handle_obj_dict(&frame.payload, &mut lines),
            predefined::FUN_DICT    => self.handle_fun_dict(&frame.payload, &mut lines),
            predefined::USR_DICT    => self.handle_usr_dict(&frame.payload, &mut lines),
            predefined::TARGET_INFO => self.handle_target_info(&frame.payload, &mut lines),

            // ── QEP: state machine ─────────────────────────────────────────
            qep::STATE_ENTRY  => self.handle_state_entry(&frame.payload, &mut lines),
            qep::STATE_EXIT   => self.handle_state_exit(&frame.payload, &mut lines),
            qep::STATE_INIT   => self.handle_state_init(&frame.payload, &mut lines),
            qep::INIT_TRAN    => self.handle_init_tran(&frame.payload, &mut lines),
            qep::INTERN_TRAN  => self.handle_intern_tran(&frame.payload, &mut lines),
            qep::TRAN         => self.handle_tran(&frame.payload, &mut lines),
            qep::IGNORED      => self.handle_ignored(&frame.payload, &mut lines),
            qep::DISPATCH     => self.handle_dispatch(&frame.payload, &mut lines),
            qep::UNHANDLED    => self.handle_unhandled(&frame.payload, &mut lines),
            qep::TRAN_HIST    => self.handle_tran_hist(&frame.payload, &mut lines),

            // ── QF: active object ─────────────────────────────────────────
            qf::ACTIVE_DEFER         => self.handle_ao_defer_recall(&frame.payload, "AO-Defer ", &mut lines),
            qf::ACTIVE_RECALL        => self.handle_ao_defer_recall(&frame.payload, "AO-Rcall ", &mut lines),
            qf::ACTIVE_SUBSCRIBE     => self.handle_ao_subscribe(&frame.payload, &mut lines),
            qf::ACTIVE_UNSUBSCRIBE   => self.handle_ao_unsubscribe(&frame.payload, &mut lines),
            qf::ACTIVE_POST          => self.handle_ao_post(&frame.payload, "AO-Post ", &mut lines),
            qf::ACTIVE_POST_LIFO     => self.handle_ao_post(&frame.payload, "AO-PostL", &mut lines),
            qf::ACTIVE_GET           => self.handle_ao_get(&frame.payload, &mut lines),
            qf::ACTIVE_GET_LAST      => self.handle_ao_get_last(&frame.payload, &mut lines),
            qf::ACTIVE_POST_ATTEMPT  => self.handle_ao_post(&frame.payload, "AO-PostA", &mut lines),

            // ── QF: event queues ─────────────────────────────────────────
            qf::EQUEUE_INIT          => self.handle_equeue_init(&frame.payload, &mut lines),
            qf::EQUEUE_POST          => self.handle_equeue_post(&frame.payload, "EQ-Post ", &mut lines),
            qf::EQUEUE_POST_LIFO     => self.handle_equeue_post(&frame.payload, "EQ-PostL", &mut lines),
            qf::EQUEUE_GET           => self.handle_equeue_get(&frame.payload, "EQ-Get  ", &mut lines),
            qf::EQUEUE_GET_LAST      => self.handle_equeue_get(&frame.payload, "EQ-GetL ", &mut lines),
            qf::EQUEUE_POST_ATTEMPT  => self.handle_equeue_post(&frame.payload, "EQ-PostA", &mut lines),

            // ── QF: memory pool ───────────────────────────────────────────
            qf::MPOOL_INIT        => self.handle_mpool_init(&frame.payload, &mut lines),
            qf::MPOOL_GET         => self.handle_mpool_get(&frame.payload, &mut lines),
            qf::MPOOL_PUT         => self.handle_mpool_put(&frame.payload, &mut lines),
            qf::MPOOL_GET_ATTEMPT => self.handle_mpool_get_labeled(&frame.payload, "MP-GetA ", &mut lines),

            // ── QF: event lifecycle ───────────────────────────────────────
            qf::PUBLISH    => self.handle_qf_publish(&frame.payload, &mut lines),
            qf::NEW_REF    => self.handle_qf_evt_ref(&frame.payload, "New-Ref ", &mut lines),
            qf::NEW        => self.handle_qf_new(&frame.payload, &mut lines),
            qf::GC_ATTEMPT => self.handle_qf_gc(&frame.payload, "QF-gcA  ", &mut lines),
            qf::GC         => self.handle_qf_gc(&frame.payload, "QF-gc   ", &mut lines),
            qf::TICK       => self.handle_qf_tick(&frame.payload, &mut lines),
            qf::DELETE_REF => self.handle_qf_evt_ref(&frame.payload, "QF-DelRf", &mut lines),

            // ── QF: critical section / ISR ────────────────────────────────
            qf::CRIT_ENTRY => self.handle_crit(&frame.payload, "QF-CritE", &mut lines),
            qf::CRIT_EXIT  => self.handle_crit(&frame.payload, "QF-CritX", &mut lines),
            qf::ISR_ENTRY  => self.handle_isr(&frame.payload, "QF-IsrE ", &mut lines),
            qf::ISR_EXIT   => self.handle_isr(&frame.payload, "QF-IsrX ", &mut lines),

            // ── QF: time events ───────────────────────────────────────────
            time_evt::ARM            => self.handle_time_evt_arm(&frame.payload, &mut lines),
            time_evt::AUTO_DISARM    => self.handle_time_evt_auto_disarm(&frame.payload, &mut lines),
            time_evt::DISARM_ATTEMPT => self.handle_time_evt_disarm_attempt(&frame.payload, &mut lines),
            time_evt::DISARM         => self.handle_time_evt_disarm(&frame.payload, &mut lines),
            time_evt::REARM          => self.handle_time_evt_rearm(&frame.payload, &mut lines),
            time_evt::POST           => self.handle_time_evt_post(&frame.payload, &mut lines),

            // ── Scheduler ─────────────────────────────────────────────────
            sched::LOCK   => self.handle_sched_lock(&frame.payload, &mut lines),
            sched::UNLOCK => self.handle_sched_unlock(&frame.payload, &mut lines),
            sched::NEXT   => self.handle_sched_next(&frame.payload, &mut lines),
            sched::IDLE   => self.handle_sched_idle(&frame.payload, &mut lines),

            // ── Infrastructure / test back-channel ────────────────────────
            infra::TEST_PAUSED => lines.push("           TstPause".to_string()),
            infra::TEST_PROBE  => self.handle_test_probe(&frame.payload, &mut lines),
            infra::QUERY_DATA  => self.handle_query_data(&frame.payload, &mut lines),
            infra::PEEK_DATA   => self.handle_peek_data(&frame.payload, &mut lines),
            infra::ASSERT_FAIL => self.handle_assert_fail(&frame.payload, &mut lines),
            infra::QF_RUN      => lines.push("           QF RUN".to_string()),

            // ── QSPY back-channel ─────────────────────────────────────────
            65 => lines.push(format!(
                "           Trg-Done rec={}", frame.payload.first().copied().unwrap_or(0)
            )),
            66 => self.handle_rx_status(&frame.payload, &mut lines),

            // ── User records ──────────────────────────────────────────────
            rec if rec >= 100 || self.dict.users.contains_key(&rec)
                => self.handle_user_record(rec, &frame.payload, &mut lines),

            _ => {}
        }

        if lines.is_empty() {
            lines.push(self.fallback_line(frame));
        }
        lines
    }

    // ── Dictionary string helpers ─────────────────────────────────────────────

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
        let sig32 = signal as u32;
        if let Some(name) = self.dict.signals.get(&(sig32, obj))
            .or_else(|| self.dict.signals.get(&(sig32, 0)))
        {
            return name.clone();
        }
        TargetSizes::fmt_addr(signal, self.sizes.signal_size)
    }

    // ── Predefined record handlers ────────────────────────────────────────────

    fn handle_sig_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(signal), Some(object), Some(name)) = (
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_c_string(),
        ) {
            self.dict.signals.insert((signal as u32, object), name.clone());
            lines.push(format!(
                "           Sig-Dict {signal:#010X},Obj={obj}->{name}",
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
                "           Obj-Dict {addr}->{name}",
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
                "           Fun-Dict {addr}->{name}",
                addr = TargetSizes::fmt_addr(addr, self.sizes.fun_ptr_size)
            ));
        }
    }

    fn handle_usr_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(id), Some(name)) = (cur.read_u8(), cur.read_c_string()) {
            self.dict.users.insert(id, name.clone());
            lines.push(format!("           Usr-Dict {id:03}->{name}"));
        }
    }

    fn handle_enum_dict(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(val), Some(grp), Some(name)) =
            (cur.read_u8(), cur.read_u8(), cur.read_c_string())
        {
            self.dict.enums.insert((grp, val), name.clone());
            lines.push(format!("           Enum-Dict grp={grp} {val}->{name}"));
        }
    }

    fn handle_target_info(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (
            Some(reset), Some(version),
            Some(sig_evt), Some(eq_te), Some(mp_sizes), Some(ptr_sizes),
            Some(time_size), Some(max_active), Some(max_pool_tick),
            Some(second), Some(minute), Some(hour),
            Some(day), Some(month), Some(year),
        ) = (
            cur.read_u8(),  cur.read_u16(), cur.read_u8(), cur.read_u8(),
            cur.read_u8(),  cur.read_u8(),  cur.read_u8(), cur.read_u8(),
            cur.read_u8(),  cur.read_u8(),  cur.read_u8(), cur.read_u8(),
            cur.read_u8(),  cur.read_u8(),  cur.read_u8(),
        ) {
            let stamp = format!("{day:02}{month:02}{year:02}_{hour:02}{minute:02}{second:02}");
            let reset_tag = if reset == 0xFF { "RST" } else { "INF" };
            lines.push(format!("########## Trg-{reset_tag}  QP-Ver={version},Build={stamp}"));
            lines.push(format!(
                "           Cfg Sig/Evt={sig_evt:#04X} Eq/Te={eq_te:#04X} Mp={mp_sizes:#04X} \
                 Ptr={ptr_sizes:#04X} Time={time_size:#04X} Active={max_active} \
                 Pools/Ticks={max_pool_tick:#04X}"
            ));
            self.sizes.update_from_target_info(payload);
        }
    }

    fn handle_rx_status(&self, payload: &[u8], lines: &mut Vec<String>) {
        if let Some(&b) = payload.first() {
            if b & 0x80 != 0 {
                lines.push(format!("           QS-RX Err={:#04X}", b & 0x7F));
            } else {
                lines.push(format!("           QS-RX Ack rec={b}"));
            }
        }
    }

    // ── QEP handlers ─────────────────────────────────────────────────────────

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
                "{ts:010} =>Ignore Obj={},Sig={},State={}",
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
                "===RTC===> =>UnHndl Obj={},Sig={},State={}",
                self.obj_str(obj), self.sig_str(signal, obj), self.fun_str(state)
            ));
        }
    }

    /// `QS_QEP_TRAN_HIST` (55): [obj | src | tgt] — no timestamp, RTC step
    fn handle_tran_hist(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(obj), Some(src), Some(tgt)) = (
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
            cur.read_sized(self.sizes.fun_ptr_size),
        ) {
            lines.push(format!(
                "===RTC===> St-Hist  Obj={},State={}->{}",
                self.obj_str(obj), self.fun_str(src), self.fun_str(tgt)
            ));
        }
    }

    // ── QF: active object handlers ────────────────────────────────────────────

    /// `QS_QF_ACTIVE_DEFER` (10) / `QS_QF_ACTIVE_RECALL` (11): [ts | ao | eq | sig | pool | ref]
    fn handle_ao_defer_recall(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(ao), Some(eq), Some(sig), Some(pool), Some(rref)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_u8(), cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} {label} Obj={},Que={},Evt<Sig={},Pool={pool},Ref={rref}>",
                self.obj_str(ao), self.obj_str(eq), self.sig_str(sig, ao)
            ));
        }
    }

    /// `QS_QF_ACTIVE_SUBSCRIBE` (12): [ts | sig | ao]
    fn handle_ao_subscribe(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(ao)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} AO-Subsc Obj={},Sig={}",
                self.obj_str(ao), self.sig_str(sig, ao)
            ));
        }
    }

    /// `QS_QF_ACTIVE_UNSUBSCRIBE` (13): [ts | sig | ao]
    fn handle_ao_unsubscribe(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(ao)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
        ) {
            lines.push(format!(
                "{ts:010} AO-Unsub Obj={},Sig={}",
                self.obj_str(ao), self.sig_str(sig, ao)
            ));
        }
    }

    /// `QS_QF_ACTIVE_POST_FIFO/LIFO` (14/15): [ts | sig | sdr | ao | pool | ref | free | min]
    fn handle_ao_post(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(sdr), Some(ao),
                Some(pool), Some(rref), Some(free), Some(min)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(), cur.read_u8(),
            cur.read_sized(self.sizes.equeue_ctr),
            cur.read_sized(self.sizes.equeue_ctr),
        ) {
            lines.push(format!(
                "{ts:010} {label} Sdr={},Obj={},Evt<Sig={},Pool={pool},Ref={rref}>,Que<Free={free},Min={min}>",
                self.obj_str(sdr), self.obj_str(ao), self.sig_str(sig, ao)
            ));
        }
    }

    /// `QS_QF_ACTIVE_GET` (16): [ts | sig | ao | pool | ref | free]
    fn handle_ao_get(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(ao), Some(pool), Some(rref), Some(free)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(), cur.read_u8(),
            cur.read_sized(self.sizes.equeue_ctr),
        ) {
            lines.push(format!(
                "{ts:010} AO-Get   Obj={},Evt<Sig={},Pool={pool},Ref={rref}>,Que<Free={free}>",
                self.obj_str(ao), self.sig_str(sig, ao)
            ));
        }
    }

    /// `QS_QF_ACTIVE_GET_LAST` (17): [ts | sig | ao | pool | ref]
    fn handle_ao_get_last(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(ao), Some(pool), Some(rref)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(), cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} AO-GetL  Obj={},Evt<Sig={},Pool={pool},Ref={rref}>",
                self.obj_str(ao), self.sig_str(sig, ao)
            ));
        }
    }

    // ── QF: event queue / memory pool init handlers ───────────────────────────

    /// `QS_QF_EQUEUE_INIT` (18): [ts | eq | len: equeue_ctr]
    fn handle_equeue_init(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(eq), Some(len)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.equeue_ctr),
        ) {
            lines.push(format!(
                "{ts:010} EQ-Init  Obj={},Len={len}",
                self.obj_str(eq)
            ));
            if !cur.is_empty() {
                lines.push(format!(
                    "           !! {} bytes unused in rec={:#04X}",
                    cur.remaining(), qf::EQUEUE_INIT
                ));
            }
        }
    }

    /// `QS_QF_MPOOL_INIT` (23): [ts | mp | n_free: mpool_ctr | n_min: mpool_ctr]
    fn handle_mpool_init(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(mp), Some(n_free), Some(n_min)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.mpool_ctr),
            cur.read_sized(self.sizes.mpool_ctr),
        ) {
            lines.push(format!(
                "{ts:010} MP-Init  Obj={},NFree={n_free},NMin={n_min}",
                self.obj_str(mp)
            ));
            if !cur.is_empty() {
                lines.push(format!(
                    "           !! {} bytes unused in rec={:#04X}",
                    cur.remaining(), qf::MPOOL_INIT
                ));
            }
        }
    }

    // ── QF: event queue handlers ──────────────────────────────────────────────

    /// `QS_QF_EQUEUE_POST_FIFO/LIFO` (19/20): [ts | sig | eq | pool | ref | free | min]
    fn handle_equeue_post(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(eq),
                Some(pool), Some(rref), Some(free), Some(min)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(), cur.read_u8(),
            cur.read_sized(self.sizes.equeue_ctr),
            cur.read_sized(self.sizes.equeue_ctr),
        ) {
            lines.push(format!(
                "{ts:010} {label} Obj={},Evt<Sig={},Pool={pool},Ref={rref}>,Que<Free={free},Min={min}>",
                self.obj_str(eq), self.sig_str(sig, eq)
            ));
        }
    }

    /// `QS_QF_EQUEUE_GET / GET_LAST` (21/22): [ts | sig | eq | pool | ref | free]
    fn handle_equeue_get(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(eq), Some(pool), Some(rref), Some(free)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_u8(), cur.read_u8(),
            cur.read_sized(self.sizes.equeue_ctr),
        ) {
            lines.push(format!(
                "{ts:010} {label} Obj={},Evt<Sig={},Pool={pool},Ref={rref}>,Que<Free={free}>",
                self.obj_str(eq), self.sig_str(sig, eq)
            ));
        }
    }

    // ── QF: memory pool handlers ──────────────────────────────────────────────

    /// `QS_QF_MPOOL_GET` (24) / `QS_QF_MPOOL_GET_ATTEMPT` (47): [ts | mp | free | min]
    fn handle_mpool_get(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        self.handle_mpool_get_labeled(payload, "MP-Get  ", lines);
    }

    fn handle_mpool_get_labeled(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(mp), Some(free), Some(min)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.mpool_ctr),
            cur.read_sized(self.sizes.mpool_ctr),
        ) {
            lines.push(format!(
                "{ts:010} {label} Obj={},Free={free},Min={min}",
                self.obj_str(mp)
            ));
        }
    }

    /// `QS_QF_MPOOL_PUT` (25): [ts | mp | free]
    fn handle_mpool_put(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(mp), Some(free)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.mpool_ctr),
        ) {
            lines.push(format!(
                "{ts:010} MP-Put   Obj={},Free={free}",
                self.obj_str(mp)
            ));
        }
    }

    // ── QF: event lifecycle handlers ──────────────────────────────────────────

    /// `QS_QF_PUBLISH` (26): [ts | sdr | sig | pool | ref]
    fn handle_qf_publish(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sdr), Some(sig), Some(pool), Some(rref)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.obj_ptr_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_u8(), cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} QF-Pub   Sdr={},Evt<Sig={},Pool={pool},Ref={rref}>",
                self.obj_str(sdr), self.sig_str(sig, 0)
            ));
        }
    }

    /// `QS_QF_NEW` (28): [ts | evt_size | sig]
    fn handle_qf_new(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(size), Some(sig)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.event_size),
            cur.read_sized(self.sizes.signal_size),
        ) {
            lines.push(format!(
                "{ts:010} QF-New   Sig={},Size={size}",
                self.sig_str(sig, 0)
            ));
        }
    }

    /// `QS_QF_GC_ATTEMPT` (29) / `QS_QF_GC` (30): [ts | sig | pool | ref]
    fn handle_qf_gc(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(pool), Some(rref)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_u8(), cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} {label} Evt<Sig={},Pool={pool},Ref={rref}>",
                self.sig_str(sig, 0)
            ));
        }
    }

    /// `QS_QF_TICK` (31): [ts | rate]
    fn handle_qf_tick(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(rate)) = (cur.read_sized(self.sizes.time_size), cur.read_u8()) {
            lines.push(format!("{ts:010} QF-Tick  Rate={rate}"));
        }
    }

    /// `QS_QF_NEW_REF` (27) / `QS_QF_DELETE_REF` (38): [ts | sig | pool | ref]
    fn handle_qf_evt_ref(&mut self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(sig), Some(pool), Some(rref)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.signal_size),
            cur.read_u8(), cur.read_u8(),
        ) {
            lines.push(format!(
                "{ts:010} {label} Evt<Sig={},Pool={pool},Ref={rref}>",
                self.sig_str(sig, 0)
            ));
        }
    }

    /// `QS_TR_CRIT_ENTRY` (39) / `QS_TR_CRIT_EXIT` (40): [ts | nesting]
    fn handle_crit(&self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(nesting)) = (cur.read_sized(self.sizes.time_size), cur.read_u8()) {
            lines.push(format!("{ts:010} {label} Nesting={nesting}"));
        }
    }

    /// `QS_TR_ISR_ENTRY` (41) / `QS_TR_ISR_EXIT` (42): [ts | nesting | prio]
    fn handle_isr(&self, payload: &[u8], label: &str, lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(nesting), Some(prio)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_u8(), cur.read_u8(),
        ) {
            lines.push(format!("{ts:010} {label} Nesting={nesting},Pri={prio}"));
        }
    }

    // ── Time event handlers ───────────────────────────────────────────────────

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

    fn handle_time_evt_rearm(&mut self, payload: &[u8], lines: &mut Vec<String>) {
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
                "{ts:010} TE{rate}-Rarm Obj={},AO={},Tim={remaining},Int={interval}",
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

    // ── Scheduler handlers ────────────────────────────────────────────────────

    fn handle_sched_lock(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev), Some(new)) =
            (cur.read_sized(self.sizes.time_size), cur.read_u8(), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Lock Ceil={prev}->{new}"));
        }
    }

    fn handle_sched_unlock(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(prev), Some(new)) =
            (cur.read_sized(self.sizes.time_size), cur.read_u8(), cur.read_u8())
        {
            lines.push(format!("{ts:010} Sch-Unlk Ceil={new}->{prev}"));
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

    // ── Infrastructure / test handlers ───────────────────────────────────────

    /// `QS_TEST_PROBE_GET` (59): [ts | api_fun | data_u32]
    fn handle_test_probe(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(api), Some(data)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_sized(self.sizes.fun_ptr_size),
            cur.read_u32(),
        ) {
            lines.push(format!(
                "{ts:010} TstProbe Fun={},Data={data:#010X}",
                self.fun_str(api)
            ));
        }
    }

    /// `QS_QUERY_DATA` (67): [ts | kind | obj]
    fn handle_query_data(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(kind), Some(obj)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_u8(),
            cur.read_sized(self.sizes.obj_ptr_size),
        ) {
            let kind_str = match kind {
                0 => "SM",  1 => "AO",  2 => "MP",
                3 => "EQ",  4 => "TE",  5 => "AP",
                _ => "??",
            };
            lines.push(format!(
                "{ts:010} Query-{kind_str} Obj={}",
                self.obj_str(obj)
            ));
        }
    }

    /// `QS_PEEK_DATA` (68): [offset_u16 | size | data...]
    fn handle_peek_data(&self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(offset), Some(size)) = (cur.read_u16(), cur.read_u8()) {
            let data = cur.read_bytes(size as usize).unwrap_or(&[]);
            lines.push(format!(
                "           Trg-Peek Offset={offset},Size={size},Data={}",
                hex_bytes(data)
            ));
        }
    }

    /// `QS_ASSERT_FAIL` (69): [ts | id_u16 | module_str]
    fn handle_assert_fail(&mut self, payload: &[u8], lines: &mut Vec<String>) {
        let mut cur = Cursor::new(payload);
        if let (Some(ts), Some(id), Some(module)) = (
            cur.read_sized(self.sizes.time_size),
            cur.read_u16(),
            cur.read_c_string(),
        ) {
            lines.push(format!("{ts:010} =ASSERT= Mod={module},Loc={id}"));
        }
    }

    // ── User record handler ───────────────────────────────────────────────────

    fn handle_user_record(&mut self, record: u8, payload: &[u8], lines: &mut Vec<String>) {
        let name = self.dict.users.get(&record).cloned()
            .unwrap_or_else(|| format!("USR({record})"));

        let mut cur = Cursor::new(payload);
        let ts = match cur.read_sized(self.sizes.time_size) {
            Some(v) => v,
            None => {
                lines.push(format!("           {name} payload={}", hex_bytes(payload)));
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

        // Per-record pretty-printers that override the generic field list.
        if name == "LORA_TX_PKT" {
            if let Some(line) = format_lora_tx_pkt(&values) {
                lines.push(format!("{ts:010} {line}"));
                return;
            }
        }
        if name == "SWM_ACTOR_TRAN" {
            if let Some(line) = format_swm_actor_tran(&values) {
                lines.push(format!("{ts:010} {line}"));
                return;
            }
        }
        if name == "SWM_SESSION_TRAN" {
            if let Some(line) = format_swm_session_tran(&values) {
                lines.push(format!("{ts:010} {line}"));
                return;
            }
        }

        lines.push(format!("{ts:010} {name} {}", values.join(" ")));
    }

    fn fallback_line(&self, frame: &QsFrame) -> String {
        format!(
            "           rec={:#04X} len={} payload={}",
            frame.record_type,
            frame.payload.len(),
            hex_bytes(&frame.payload)
        )
    }

    // ── Dictionary persistence ────────────────────────────────────────────────

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
        for ((grp, val), name) in &self.dict.enums {
            writeln!(w, "ENUM {grp} {val} {name}")?;
        }
        Ok(())
    }

    pub fn load_dictionaries(&mut self, path: &Path) -> io::Result<()> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
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
                    if let (Ok(s), Ok(o)) = (sig.parse::<u32>(), parse_addr(obj)) {
                        self.dict.signals.insert((s, o), (*name).to_owned());
                    }
                }
                ["USR", id, name] => {
                    if let Ok(i) = id.parse::<u8>() {
                        self.dict.users.insert(i, (*name).to_owned());
                    }
                }
                ["ENUM", grp, val, name] => {
                    if let (Ok(g), Ok(v)) = (grp.parse::<u8>(), val.parse::<u8>()) {
                        self.dict.enums.insert((g, v), (*name).to_owned());
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
    signals:   HashMap<(u32, u64), String>,
    users:     HashMap<u8, String>,
    /// Keyed by (group, value).
    enums:     HashMap<(u8, u8), String>,
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

// ── Pretty-printers for known user records ────────────────────────────────────

fn format_lora_tx_pkt(values: &[String]) -> Option<String> {
    if values.len() < 6 { return None; }

    let freq_hz: u64 = values[0].parse().ok()?;
    let sf:  u8 = values[1].parse().ok()?;
    let bw:  u8 = values[2].parse().ok()?;
    let cr:  u8 = values[3].parse().ok()?;
    let pwr: u8 = values[4].parse().ok()?;

    let hex_str = values[5].strip_prefix("mem:")?;
    if hex_str.len() < 2 { return None; }

    let frame_bytes: Vec<u8> = (0..hex_str.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex_str[i..i+2], 16).ok())
        .collect();

    let bw_khz = match bw { 0 => "125", 1 => "250", 2 => "500", _ => "?" };
    let cr_str = match cr { 0 => "4/5", 1 => "4/6", 2 => "4/7", 3 => "4/8", _ => "?" };

    let lorawan = if frame_bytes.len() >= 8 {
        let mhdr  = frame_bytes[0];
        let m_type = (mhdr >> 5) & 0x07;
        let mtype_str = match m_type {
            0 => "JoinReq", 1 => "JoinAcc", 2 => "UnconfUp", 3 => "UnconfDn",
            4 => "ConfUp",  5 => "ConfDn",  _ => "Prop",
        };
        let dev_addr = u32::from_le_bytes([
            frame_bytes[1], frame_bytes[2], frame_bytes[3], frame_bytes[4],
        ]);
        let fctrl = frame_bytes[5];
        let fcnt  = u16::from_le_bytes([frame_bytes[6], frame_bytes[7]]);
        let fopts_len = (fctrl & 0x0F) as usize;
        let has_fport = frame_bytes.len() > 8 + fopts_len + 4;
        let fport = if has_fport { Some(frame_bytes[8 + fopts_len]) } else { None };
        let payload_len = frame_bytes.len()
            .saturating_sub(8 + fopts_len + if has_fport { 1 } else { 0 } + 4);

        if let Some(fp) = fport {
            format!(" | {mtype_str} DevAddr={dev_addr:#010X} FCnt={fcnt} FPort={fp} FRMPayload={payload_len}B MIC={:02X}{:02X}{:02X}{:02X}",
                frame_bytes[frame_bytes.len()-4], frame_bytes[frame_bytes.len()-3],
                frame_bytes[frame_bytes.len()-2], frame_bytes[frame_bytes.len()-1])
        } else {
            format!(" | {mtype_str} DevAddr={dev_addr:#010X} FCnt={fcnt}")
        }
    } else {
        String::new()
    };

    Some(format!(
        "LORA_TX_PKT {:.3}MHz SF{sf} BW{bw_khz}kHz CR{cr_str} +{pwr}dBm frame[{}B]:{lorawan}",
        freq_hz as f64 / 1_000_000.0,
        frame_bytes.len(),
    ))
}

// ── SWM pretty-printers ───────────────────────────────────────────────────────

fn actor_mode_name(v: u8) -> &'static str {
    match v {
        0 => "Boot", 1 => "Idle", 2 => "Sampling", 3 => "Processing",
        4 => "Transmitting", 5 => "Sleeping", 6 => "Service", 7 => "Fault", _ => "?",
    }
}

fn swm_signal_name(v: u16) -> &'static str {
    match v {
        0  => "Boot",            1  => "Tick",           2  => "Fault",
        3  => "ConfigLoad",      4  => "ConfigLoaded",   5  => "ConfigUpdate",
        6  => "ConfigCommitted", 7  => "SampleRequest",  8  => "SampleReady",
        9  => "LevelComputed",   10 => "TxRequest",      11 => "TxDone",
        12 => "RxFrame",         13 => "AckTimeout",     14 => "MacSlot",
        15 => "CommandReceived", 16 => "CommandAccepted",17 => "CommandRejected",
        18 => "CommandApplied",  19 => "FeedbackChanged",20 => "HealthRequest",
        21 => "HealthReport",    22 => "MaintenanceEnter",23=>"MaintenanceExit",
        24 => "FotaPolicy",      25 => "FotaManifest",  26 => "FotaChunk",
        27 => "FotaApply",       28 => "FotaStatus",    _ => "?",
    }
}

fn session_state_name(v: u8) -> &'static str {
    match v {
        0 => "Unbound", 1 => "BoundIdle", 2 => "TxPending", 3 => "TxActive",
        4 => "WaitAck", 5 => "RxWindow",  6 => "Backoff",   7 => "ServiceWindow",
        8 => "Fault",   _ => "?",
    }
}

/// `values`: [actor_name, from_u8, to_u8, signal_u16]
fn format_swm_actor_tran(values: &[String]) -> Option<String> {
    if values.len() < 4 { return None; }
    let actor  = &values[0];
    let from: u8  = values[1].parse().ok()?;
    let to:   u8  = values[2].parse().ok()?;
    let sig:  u16 = values[3].parse().ok()?;
    Some(format!(
        "SWM_ACTOR_TRAN {actor} {}→{} [{}]",
        actor_mode_name(from), actor_mode_name(to), swm_signal_name(sig),
    ))
}

/// `values`: [actor_name, from_u8, to_u8, signal_u16]
fn format_swm_session_tran(values: &[String]) -> Option<String> {
    if values.len() < 4 { return None; }
    let actor  = &values[0];
    let from: u8  = values[1].parse().ok()?;
    let to:   u8  = values[2].parse().ok()?;
    let sig:  u16 = values[3].parse().ok()?;
    Some(format!(
        "SWM_SESSION_TRAN {actor} {}→{} [{}]",
        session_state_name(from), session_state_name(to), swm_signal_name(sig),
    ))
}
