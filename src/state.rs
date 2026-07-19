// ── Exo State: ponte epistemico Exo → v2 ──
// Scritto dopo ogni test run significativo.
// Nova v2 legge questo file invece di indovinare da log vecchi.
//
// Formato: JSON leggibile da qualsiasi LLM o script.

use crate::serial_putc;

static mut EXO_STATE_BUF: [u8; 2048] = [0; 2048];
static mut EXO_STATE_LEN: usize = 0;

pub fn publish(
    version: &str,
    tick: u32,
    attractor_events: u32,
    attractor_sim_mean: f32,
    unique_patterns: u32,
    familiarity_mean: f32,
    pd_index: f32,
    sediment_active: bool,
) {
    unsafe {
        let buf = &mut *(&raw mut EXO_STATE_BUF);
        let mut pos = 0;

        macro_rules! w {
            ($s:expr) => {
                let s = $s;
                for &b in s.as_bytes() {
                    if pos < buf.len() {
                        buf[pos] = b;
                        pos += 1;
                    }
                }
            };
        }

        w!("{\n");
        w!("  \"exo_version\": \""); w!(version); w!("\",\n");
        w!("  \"last_tick\": "); write_num(&mut pos, tick); w!(",\n");
        w!("  \"attractor\": {\n");
        w!("    \"events\": "); write_num(&mut pos, attractor_events); w!(",\n");
        w!("    \"sim_mean\": "); write_f32_to(&mut pos, attractor_sim_mean); w!("\n");
        w!("  },\n");
        w!("  \"memory\": {\n");
        w!("    \"unique_patterns\": "); write_num(&mut pos, unique_patterns); w!(",\n");
        w!("    \"familiarity_mean\": "); write_f32_to(&mut pos, familiarity_mean); w!("\n");
        w!("  },\n");
        w!("  \"path_dependency_index\": "); write_f32_to(&mut pos, pd_index); w!(",\n");
        w!("  \"sedimentation\": ");
        w!(if sediment_active { "true" } else { "false" });
        w!("\n");
        w!("}\n");

        EXO_STATE_LEN = pos;

        // Echo to debugcon for capture
        for i in 0..pos {
            serial_putc(buf[i]);
        }
        serial_putc(b'\n');
    }
}

fn write_num(pos: &mut usize, n: u32) {
    unsafe {
        let buf = &mut *(&raw mut EXO_STATE_BUF);
        if n == 0 {
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
            return;
        }
        let mut tmp = [0u8; 12];
        let mut i = 12;
        let mut v = n;
        while v > 0 {
            i -= 1;
            tmp[i] = (v % 10) as u8 + b'0';
            v /= 10;
        }
        for &b in &tmp[i..] {
            if *pos < buf.len() { buf[*pos] = b; *pos += 1; }
        }
    }
}

fn write_f32_to(pos: &mut usize, val: f32) {
    unsafe {
        let buf = &mut *(&raw mut EXO_STATE_BUF);
        let sign = if val < 0.0 { -1.0 } else { 1.0 };
        let v = (val.abs() * 10000.0 + 0.5) as u32;
        let int_part = v / 10000;
        let frac_part = v % 10000;
        if sign < 0.0 {
            if *pos < buf.len() { buf[*pos] = b'-'; *pos += 1; }
        }
        write_num(pos, int_part);
        if *pos < buf.len() { buf[*pos] = b'.'; *pos += 1; }
        if frac_part < 10 {
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
        } else if frac_part < 100 {
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
        } else if frac_part < 1000 {
            if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
        }
        write_num(pos, frac_part);
    }
}
