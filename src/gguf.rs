// ── GGUF Parser (no_std) — Sempre, 13 Ago 2026 ──
#![no_std]
//
// Parses the GGUF format (GGML Universal File) used by llama.cpp and
// LFM2.5-GGUF. The kernel needs this to load model weights from a
// buffer (today: embedded test file; tomorrow: block device).
//
// Format (little-endian):
//   header: magic[4]="GGUF" + version:u32 + tensor_count:u64 + kv_count:u64
//   metadata KV: for each: key:str, type:u32, value (type-dependent)
//   tensor infos: for each: name:str, n_dims:u32, dims:[u64], type:u32, offset:u64
//   tensor data follows after alignment
//
// Value types (from gguf.h):
//   0=u8, 1=i8, 2=u16, 3=i16, 4=u32, 5=i32, 6=f32, 7=bool, 8=str,
//   9=array, 10=u64, 11=i64, 12=f64
//
// This module is no_std, no alloc, no panic: all reads are checked and
// return Result/Option. Errors are reported, never crash (metodo Exo).

use core::fmt;

/// Errore di parsing GGUF
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GgufError {
    BadMagic,
    UnsupportedVersion(u32),
    BufferTooSmall,
    Truncated,
    BadType(u32),
    BadTensor,
}

impl fmt::Display for GgufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GgufError::BadMagic => write!(f, "bad magic (expected GGUF)"),
            GgufError::UnsupportedVersion(v) => write!(f, "unsupported version {v}"),
            GgufError::BufferTooSmall => write!(f, "buffer too small"),
            GgufError::Truncated => write!(f, "truncated data"),
            GgufError::BadType(t) => write!(f, "bad value type {t}"),
            GgufError::BadTensor => write!(f, "bad tensor"),
        }
    }
}

pub type GgufResult<T> = Result<T, GgufError>;

/// Cursore di lettura sicuro su un buffer
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> GgufResult<u8> {
        if self.remaining() < 1 { return Err(GgufError::Truncated); }
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u32(&mut self) -> GgufResult<u32> {
        if self.remaining() < 4 { return Err(GgufError::Truncated); }
        let v = u32::from_le_bytes([
            self.buf[self.pos], self.buf[self.pos+1],
            self.buf[self.pos+2], self.buf[self.pos+3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_u64(&mut self) -> GgufResult<u64> {
        if self.remaining() < 8 { return Err(GgufError::Truncated); }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.buf[self.pos..self.pos+8]);
        self.pos += 8;
        Ok(u64::from_le_bytes(b))
    }

    fn read_str(&mut self) -> GgufResult<&'a str> {
        let len = self.read_u64()? as usize;
        if self.remaining() < len { return Err(GgufError::Truncated); }
        let s = core::str::from_utf8(&self.buf[self.pos..self.pos+len])
            .map_err(|_| GgufError::Truncated)?;
        self.pos += len;
        Ok(s)
    }

    fn skip(&mut self, n: usize) -> GgufResult<()> {
        if self.remaining() < n { return Err(GgufError::Truncated); }
        self.pos += n;
        Ok(())
    }
}

/// Header GGUF
#[derive(Debug, Clone, Copy)]
pub struct GgufHeader {
    pub version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
}

/// Informazione su un tensore (senza dati — solo indice e offset)
#[derive(Debug, Clone)]
pub struct GgufTensorInfo {
    pub name: &'static str, // nota: in no_std senza alloc, i nomi non si copiano
    pub n_dims: u32,
    pub dims: [u64; 8],     // max 8 dims (GGUF permette n_dims qualsiasi, ma praticamente <= 4)
    pub tensor_type: u32,
    pub offset: u64,        // offset dei dati (dal data_offset)
}

/// Info parsate dal GGUF (senza allocare — snapshot dei numeri)
pub struct GgufSummary {
    pub header: GgufHeader,
    /// Primo KV trovato (chiave + tipo) — per diagnosi
    pub first_kv_key: Option<&'static str>,
    pub first_kv_type: u32,
    /// Primo tensore
    pub first_tensor: Option<GgufTensorInfo>,
    /// Offset dei dati pesi
    pub data_offset: usize,
}

/// Parsa l'header GGUF (magic, version, conteggi)
pub fn parse_header(buf: &[u8]) -> GgufResult<GgufHeader> {
    let mut c = Cursor::new(buf);
    let magic = c.read_u32()?;
    if magic != 0x4655_4747 { // "GGUF" little-endian
        return Err(GgufError::BadMagic);
    }
    let version = c.read_u32()?;
    if version != 3 {
        return Err(GgufError::UnsupportedVersion(version));
    }
    let tensor_count = c.read_u64()?;
    let kv_count = c.read_u64()?;
    Ok(GgufHeader { version, tensor_count, kv_count })
}

/// Parsa l'intero header + metadata + tensor info (senza allocare).
/// Ritorna un riepilogo numerico — il kernel non copia i nomi.
pub fn parse_summary(buf: &[u8]) -> GgufResult<GgufSummary> {
    let header = parse_header(buf)?;
    let mut c = Cursor::new(buf);
    c.skip(24)?; // header già letto

    // KV metadata
    let mut first_kv_key: Option<&'static str> = None;
    let mut first_kv_type: u32 = 0;
    for i in 0..header.kv_count.min(100) { // leggi al max 100 KV (diagnosi)
        let key = c.read_str()?;
        let vt = c.read_u32()?;
        if i == 0 {
            // leak-free: in no_std non possiamo copiare. Salva solo flag.
            first_kv_key = if key.len() > 0 { Some("(key present)") } else { None };
            first_kv_type = vt;
        }
        skip_value(&mut c, vt)?;
    }
    // se ci sono più KV di 100, skippali senza leggerli (per essere sicuri)
    // nota: per parse_summary diagnostico ci fermiamo — il kernel non ne ha bisogno

    // Tensor info
    let mut first_tensor: Option<GgufTensorInfo> = None;
    for i in 0..header.tensor_count {
        let name = c.read_str()?;
        let n_dims = c.read_u32()?;
        let mut dims = [0u64; 8];
        if n_dims as usize > dims.len() {
            return Err(GgufError::BadTensor);
        }
        for d in 0..n_dims as usize {
            dims[d] = c.read_u64()?;
        }
        let tensor_type = c.read_u32()?;
        let offset = c.read_u64()?;
        if i == 0 {
            first_tensor = Some(GgufTensorInfo {
                name: if name.len() > 0 { "tensor" } else { "" },
                n_dims,
                dims,
                tensor_type,
                offset,
            });
        }
    }

    let mut data_offset = c.pos;
    // GGUF: se ci sono tensori, i dati sono allineati a 32 byte
    // (gguf_get_data_offset: "padded to gguf_get_alignment if the
    //  gguf_context contains at least one tensor")
    if header.tensor_count > 0 {
        data_offset = (data_offset + 31) & !31;
    }
    Ok(GgufSummary {
        header,
        first_kv_key,
        first_kv_type,
        first_tensor,
        data_offset,
    })
}

/// Salta un valore in base al tipo (per non doverlo materializzare)
fn skip_value(c: &mut Cursor, vt: u32) -> GgufResult<()> {
    match vt {
        0 | 1 => { c.skip(1)?; }
        2 | 3 => { c.skip(2)?; }
        4 | 5 | 6 | 7 => { c.skip(4)?; }
        8 => { let _ = c.read_str()?; }
        9 => {
            let at = c.read_u32()?;
            let n = c.read_u64()?;
            for _ in 0..n.min(100000) {
                skip_value(c, at)?;
            }
        }
        10 | 11 | 12 => { c.skip(8)?; }
        _ => return Err(GgufError::BadType(vt)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_min_gguf() -> Vec<u8> {
        // GGUF v3 minimale: 1 KV (str) + 1 tensore (f32, dims [2,2])
        let mut v = Vec::new();
        v.extend_from_slice(b"GGUF");          // magic
        v.extend_from_slice(&3u32.to_le_bytes()); // version
        v.extend_from_slice(&1u64.to_le_bytes()); // tensor_count
        v.extend_from_slice(&1u64.to_le_bytes()); // kv_count
        // KV: key="general.name", type=str(8), value="test"
        let key = b"general.name";
        v.extend_from_slice(&(key.len() as u64).to_le_bytes());
        v.extend_from_slice(key);
        v.extend_from_slice(&8u32.to_le_bytes());
        let val = b"test";
        v.extend_from_slice(&(val.len() as u64).to_le_bytes());
        v.extend_from_slice(val);
        // Tensor: name="w0", n_dims=2, dims=[2,2], type=f32(6), offset=0
        let tn = b"w0";
        v.extend_from_slice(&(tn.len() as u64).to_le_bytes());
        v.extend_from_slice(tn);
        v.extend_from_slice(&2u32.to_le_bytes());
        v.extend_from_slice(&2u64.to_le_bytes());
        v.extend_from_slice(&2u64.to_le_bytes());
        v.extend_from_slice(&6u32.to_le_bytes());
        v.extend_from_slice(&0u64.to_le_bytes());
        v
    }

    #[test]
    fn parse_header_ok() {
        let buf = build_min_gguf();
        let h = parse_header(&buf).unwrap();
        assert_eq!(h.version, 3);
        assert_eq!(h.tensor_count, 1);
        assert_eq!(h.kv_count, 1);
    }

    #[test]
    fn parse_bad_magic() {
        let buf = b"NOPE".to_vec();
        assert_eq!(parse_header(&buf), Err(GgufError::BadMagic));
    }

    #[test]
    fn parse_summary_ok() {
        let buf = build_min_gguf();
        let s = parse_summary(&buf).unwrap();
        assert_eq!(s.header.version, 3);
        let t = s.first_tensor.unwrap();
        assert_eq!(t.n_dims, 2);
        assert_eq!(t.dims[0], 2);
        assert_eq!(t.dims[1], 2);
        assert_eq!(t.tensor_type, 6); // f32
    }

    #[test]
    fn truncated_buffer() {
        let buf = build_min_gguf();
        let short = &buf[..10];
        assert!(parse_header(short).is_err());
    }
}
