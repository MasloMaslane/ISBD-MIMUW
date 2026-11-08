use std::fs::File;
use std::io::{self, BufWriter, Write};

use crate::batch::{BATCH_SIZE, Batch, Schema};
use crate::encoding::encode_column;
use crate::format::{GroupMeta, MAGIC, VERSION, encode_footer};

pub struct Writer {
    out: BufWriter<File>,
    schema: Schema,
    pos: u64,
    groups: Vec<GroupMeta>,
}

impl Writer {
    pub fn create(path: &str, schema: Schema) -> io::Result<Writer> {
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(MAGIC)?;
        out.write_all(&[VERSION])?;
        Ok(Writer { out, schema, pos: 5, groups: Vec::new() })
    }

    // one batch = one row group in the file
    pub fn write_batch(&mut self, batch: &Batch) -> io::Result<()> {
        assert_eq!(*batch.schema, self.schema, "schema mismatch");
        let rows = batch.num_rows();
        assert!(rows <= BATCH_SIZE, "batch too big");
        if rows == 0 {
            return Ok(());
        }
        let mut chunks = Vec::new();
        for col in &batch.columns {
            let data = encode_column(col);
            self.out.write_all(&data)?;
            chunks.push((self.pos, data.len() as u64));
            self.pos += data.len() as u64;
        }
        self.groups.push(GroupMeta { rows: rows as u64, chunks });
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<()> {
        let footer = encode_footer(&self.schema, &self.groups);
        self.out.write_all(&footer)?;
        self.out.write_all(&(footer.len() as u64).to_le_bytes())?;
        self.out.write_all(MAGIC)?;
        self.out.flush()
    }
}
