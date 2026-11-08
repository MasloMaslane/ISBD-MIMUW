use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;
use std::sync::Arc;

use crate::batch::{Batch, Schema};
use crate::encoding::decode_column;
use crate::format::{GroupMeta, MAGIC, decode_footer};

pub struct Reader {
    file: File,
    pub schema: Arc<Schema>,
    groups: Vec<GroupMeta>,
    next: usize,
}

impl Reader {
    pub fn open(path: &str) -> io::Result<Reader> {
        let bad = |msg: &str| io::Error::new(io::ErrorKind::InvalidData, msg.to_string());
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        if size < 17 {
            return Err(bad("file too small"));
        }

        let mut head = [0u8; 5];
        file.read_exact_at(&mut head, 0)?;
        let mut tail = [0u8; 12];
        file.read_exact_at(&mut tail, size - 12)?;
        if &head[..4] != MAGIC || &tail[8..] != MAGIC {
            return Err(bad("not an ISBD file"));
        }

        let footer_len = u64::from_le_bytes(tail[..8].try_into().unwrap());
        if footer_len > size - 17 {
            return Err(bad("bad footer length"));
        }
        let mut footer = vec![0u8; footer_len as usize];
        file.read_exact_at(&mut footer, size - 12 - footer_len)?;
        let (schema, groups) = decode_footer(&footer)?;

        Ok(Reader { file, schema: Arc::new(schema), groups, next: 0 })
    }

    pub fn num_rows(&self) -> u64 {
        self.groups.iter().map(|g| g.rows).sum()
    }

    fn read_group(&self, g: &GroupMeta) -> io::Result<Batch> {
        let mut columns = Vec::with_capacity(g.chunks.len());
        for (i, (off, len)) in g.chunks.iter().enumerate() {
            let mut buf = vec![0u8; *len as usize];
            self.file.read_exact_at(&mut buf, *off)?;
            columns.push(decode_column(self.schema.columns[i].1, &buf, g.rows as usize));
        }
        Ok(Batch { schema: self.schema.clone(), columns })
    }
}

// next batches, only one is kept in memory
impl Iterator for Reader {
    type Item = io::Result<Batch>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.groups.len() {
            return None;
        }
        let res = self.read_group(&self.groups[self.next]);
        self.next += 1;
        Some(res)
    }
}
