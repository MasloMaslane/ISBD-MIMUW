use crate::batch::{Bitmap, Column, ColumnType, StrColumn};

pub fn write_varint(buf: &mut Vec<u8>, mut x: u64) {
    while x >= 0x80 {
        buf.push((x as u8) | 0x80);
        x >>= 7;
    }
    buf.push(x as u8);
}

pub fn read_varint(buf: &[u8], pos: &mut usize) -> u64 {
    let mut x = 0u64;
    let mut shift = 0;
    loop {
        let b = buf[*pos];
        *pos += 1;
        x |= ((b & 0x7f) as u64) << shift;
        if b < 0x80 {
            return x;
        }
        shift += 7;
    }
}

// Zigzag encoding
pub fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

pub fn unzigzag(z: u64) -> i64 {
    ((z >> 1) as i64) ^ -((z & 1) as i64)
}

fn encode_ints(vals: &[i64], out: &mut Vec<u8>) {
    let mut prev = 0i64;
    for &v in vals {
        write_varint(out, zigzag(v.wrapping_sub(prev)));
        prev = v;
    }
}

fn decode_ints(buf: &[u8], rows: usize) -> Vec<i64> {
    let mut vals = Vec::with_capacity(rows);
    let mut pos = 0;
    let mut prev = 0i64;
    for _ in 0..rows {
        prev = prev.wrapping_add(unzigzag(read_varint(buf, &mut pos)));
        vals.push(prev);
    }
    vals
}

fn encode_bools(b: &Bitmap, out: &mut Vec<u8>) {
    for w in &b.words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.truncate((b.len + 7) / 8);
}

fn decode_bools(buf: &[u8], rows: usize) -> Bitmap {
    let mut words = Vec::with_capacity((rows + 63) / 64);
    for chunk in buf.chunks(8) {
        let mut bytes = [0u8; 8];
        bytes[..chunk.len()].copy_from_slice(chunk);
        words.push(u64::from_le_bytes(bytes));
    }
    Bitmap { words, len: rows }
}

// lengths as varints, then string bytes, all compressed with lz4
fn encode_strings(s: &StrColumn, out: &mut Vec<u8>) {
    let mut raw = Vec::with_capacity(s.data.len() + s.len());
    for i in 0..s.len() {
        write_varint(&mut raw, (s.offsets[i + 1] - s.offsets[i]) as u64);
    }
    raw.extend_from_slice(&s.data);
    out.extend_from_slice(&lz4_flex::compress_prepend_size(&raw));
}

fn decode_strings(buf: &[u8], rows: usize) -> StrColumn {
    let raw = lz4_flex::decompress_size_prepended(buf).expect("bad lz4 data");
    let mut pos = 0;
    let mut offsets = Vec::with_capacity(rows + 1);
    offsets.push(0u32);
    for _ in 0..rows {
        let len = read_varint(&raw, &mut pos) as u32;
        offsets.push(offsets.last().unwrap() + len);
    }
    StrColumn { offsets, data: raw[pos..].to_vec() }
}

pub fn encode_column(col: &Column) -> Vec<u8> {
    let mut out = Vec::new();
    match col {
        Column::Int64(v) => encode_ints(v, &mut out),
        Column::Bool(b) => encode_bools(b, &mut out),
        Column::Varchar(s) => encode_strings(s, &mut out),
    }
    out
}

pub fn decode_column(t: ColumnType, buf: &[u8], rows: usize) -> Column {
    match t {
        ColumnType::Int64 => Column::Int64(decode_ints(buf, rows)),
        ColumnType::Bool => Column::Bool(decode_bools(buf, rows)),
        ColumnType::Varchar => Column::Varchar(decode_strings(buf, rows)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_zigzag() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 300);
        assert_eq!(buf, vec![0xAC, 0x02]);

        for n in [0, -1, 1, -2, 2, i64::MIN, i64::MAX] {
            assert_eq!(unzigzag(zigzag(n)), n);
        }
        assert_eq!(zigzag(-1), 1);
        assert_eq!(zigzag(2), 4);
    }
}
