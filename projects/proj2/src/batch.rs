use std::sync::Arc;

pub const BATCH_SIZE: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnType {
    Int64 = 0,
    Bool = 1,
    Varchar = 2,
}

impl ColumnType {
    pub fn from_u8(x: u8) -> Option<ColumnType> {
        match x {
            0 => Some(ColumnType::Int64),
            1 => Some(ColumnType::Bool),
            2 => Some(ColumnType::Varchar),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ColumnType::Int64 => "INT64",
            ColumnType::Bool => "BOOLEAN",
            ColumnType::Varchar => "VARCHAR",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    pub columns: Vec<(String, ColumnType)>,
}

// 1 bit per row
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Bitmap {
    pub words: Vec<u64>,
    pub len: usize,
}

impl Bitmap {
    pub fn new() -> Bitmap {
        Bitmap::default()
    }

    pub fn push(&mut self, v: bool) {
        if self.len % 64 == 0 {
            self.words.push(0);
        }
        if v {
            self.words[self.len / 64] |= 1 << (self.len % 64);
        }
        self.len += 1;
    }

    pub fn get(&self, i: usize) -> bool {
        assert!(i < self.len);
        (self.words[i / 64] >> (i % 64)) & 1 == 1
    }

    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }
}

// i-th string is data[offsets[i]..offsets[i+1]]
#[derive(Clone, Debug, PartialEq)]
pub struct StrColumn {
    pub offsets: Vec<u32>,
    pub data: Vec<u8>,
}

impl StrColumn {
    pub fn new() -> StrColumn {
        StrColumn { offsets: vec![0], data: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn get(&self, i: usize) -> &[u8] {
        &self.data[self.offsets[i] as usize..self.offsets[i + 1] as usize]
    }

    pub fn push(&mut self, s: &[u8]) {
        self.data.extend_from_slice(s);
        self.offsets.push(self.data.len() as u32);
    }

    pub fn push_concat(&mut self, a: &[u8], b: &[u8]) {
        self.data.extend_from_slice(a);
        self.data.extend_from_slice(b);
        self.offsets.push(self.data.len() as u32);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Column {
    Int64(Vec<i64>),
    Bool(Bitmap),
    Varchar(StrColumn),
}

impl Column {
    pub fn new(t: ColumnType) -> Column {
        match t {
            ColumnType::Int64 => Column::Int64(Vec::new()),
            ColumnType::Bool => Column::Bool(Bitmap::new()),
            ColumnType::Varchar => Column::Varchar(StrColumn::new()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Column::Int64(v) => v.len(),
            Column::Bool(b) => b.len,
            Column::Varchar(s) => s.len(),
        }
    }

    pub fn col_type(&self) -> ColumnType {
        match self {
            Column::Int64(_) => ColumnType::Int64,
            Column::Bool(_) => ColumnType::Bool,
            Column::Varchar(_) => ColumnType::Varchar,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub schema: Arc<Schema>,
    pub columns: Vec<Column>,
}

impl Batch {
    pub fn new(schema: Arc<Schema>) -> Batch {
        let columns = schema.columns.iter().map(|(_, t)| Column::new(*t)).collect();
        Batch { schema, columns }
    }

    pub fn num_rows(&self) -> usize {
        self.columns.first().map_or(0, |c| c.len())
    }

    pub fn col_index(&self, name: &str) -> Option<usize> {
        self.schema.columns.iter().position(|(n, _)| n == name)
    }

    pub fn int64(&self, i: usize) -> &[i64] {
        match &self.columns[i] {
            Column::Int64(v) => v,
            _ => panic!("column {} is not INT64", i),
        }
    }

    pub fn bool(&self, i: usize) -> &Bitmap {
        match &self.columns[i] {
            Column::Bool(b) => b,
            _ => panic!("column {} is not BOOLEAN", i),
        }
    }

    pub fn varchar(&self, i: usize) -> &StrColumn {
        match &self.columns[i] {
            Column::Varchar(s) => s,
            _ => panic!("column {} is not VARCHAR", i),
        }
    }

    pub fn add_column(&mut self, name: &str, col: Column) {
        assert!(self.columns.is_empty() || col.len() == self.num_rows());
        let mut schema = (*self.schema).clone();
        schema.columns.push((name.to_string(), col.col_type()));
        self.schema = Arc::new(schema);
        self.columns.push(col);
    }
}
