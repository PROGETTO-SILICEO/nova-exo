// ── Line Reader ──────────────────────────────────────────────────────────
// Accumulates serial bytes until '\n', then provides the line for parsing.

pub struct LineReader {
    buf: [u8; 64],
    len: usize,
    pub has_line: bool,
}

impl LineReader {
    pub const fn new() -> Self {
        Self {
            buf: [0u8; 64],
            len: 0,
            has_line: false,
        }
    }

    pub fn push(&mut self, byte: u8) {
        if self.has_line || self.len >= 64 {
            return;
        }
        if byte == b'\n' || byte == b'\r' {
            self.has_line = true;
        } else {
            self.buf[self.len] = byte;
            self.len += 1;
        }
    }

    pub fn consume(&mut self) {
        self.len = 0;
        self.has_line = false;
    }

    pub fn line(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn parse_line(&self) -> Option<[f32; 4]> {
        let bytes = self.line();
        let mut result = [0.0f32; 4];
        let mut idx = 0;
        let mut start = 0;
        for i in 0..=bytes.len() {
            if i == bytes.len() || bytes[i] == b',' {
                if idx >= 4 {
                    return None;
                }
                result[idx] = parse_f32(&bytes[start..i])?;
                idx += 1;
                start = i + 1;
            }
        }
        if idx == 4 { Some(result) } else { None }
    }
}

// ── Float parser (no_std) ────────────────────────────────────────────────

pub(crate) fn parse_f32(s: &[u8]) -> Option<f32> {
    if s.is_empty() {
        return None;
    }
    let neg = s[0] == b'-';
    let start = if neg || s[0] == b'+' { 1 } else { 0 };
    let mut int_part: u32 = 0;
    let mut frac_part: u32 = 0;
    let mut frac_div: u32 = 1;
    let mut i = start;
    while i < s.len() && s[i].is_ascii_digit() {
        int_part = int_part * 10 + (s[i] - b'0') as u32;
        i += 1;
    }
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            frac_part = frac_part * 10 + (s[i] - b'0') as u32;
            frac_div *= 10;
            i += 1;
        }
    }
    let val = int_part as f32 + frac_part as f32 / frac_div as f32;
    if neg { Some(-val) } else { Some(val) }
}
