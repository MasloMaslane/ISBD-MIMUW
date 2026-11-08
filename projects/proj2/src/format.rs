use std::io;

use crate::batch::{ColumnType, Schema};
use crate::encoding::{read_varint, write_varint};

pub const MAGIC: &[u8; 4] = b"ISBD";
pub const VERSION: u8 = 1;

// location of one row group in the file
#[derive(Debug, Clone)]
pub struct GroupMeta {
    pub rows: u64,
    pub chunks: Vec<(u64, u64)>, // (offset, length) for each column
}

pub fn encode_footer(schema: &Schema, groups: &[GroupMeta]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, schema.columns.len() as u64);
    for (name, t) in &schema.columns {
        write_varint(&mut out, name.len() as u64);
        out.extend_from_slice(name.as_bytes());
        out.push(*t as u8);
    }
    write_varint(&mut out, groups.len() as u64);
    for g in groups {
        write_varint(&mut out, g.rows);
        for (off, len) in &g.chunks {
            write_varint(&mut out, *off);
            write_varint(&mut out, *len);
        }
    }
    out
}

pub fn decode_footer(buf: &[u8]) -> io::Result<(Schema, Vec<GroupMeta>)> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "bad footer");
    let mut pos = 0;
    let ncols = read_varint(buf, &mut pos) as usize;
    let mut columns = Vec::with_capacity(ncols);
    for _ in 0..ncols {
        let len = read_varint(buf, &mut pos) as usize;
        let name = String::from_utf8(buf[pos..pos + len].to_vec()).map_err(|_| bad())?;
        pos += len;
        let t = ColumnType::from_u8(buf[pos]).ok_or_else(bad)?;
        pos += 1;
        columns.push((name, t));
    }
    let ngroups = read_varint(buf, &mut pos) as usize;
    let mut groups = Vec::with_capacity(ngroups);
    for _ in 0..ngroups {
        let rows = read_varint(buf, &mut pos);
        let mut chunks = Vec::with_capacity(ncols);
        for _ in 0..ncols {
            let off = read_varint(buf, &mut pos);
            let len = read_varint(buf, &mut pos);
            chunks.push((off, len));
        }
        groups.push(GroupMeta { rows, chunks });
    }
    Ok((Schema { columns }, groups))
}
