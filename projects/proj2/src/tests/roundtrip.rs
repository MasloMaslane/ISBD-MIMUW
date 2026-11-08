use std::sync::Arc;

use proj2::{Batch, Column, ColumnType, Reader, Schema, Writer};

#[test]
fn write_and_read() {
    let schema = Arc::new(Schema {
        columns: vec![
            ("a".to_string(), ColumnType::Int64),
            ("b".to_string(), ColumnType::Bool),
            ("c".to_string(), ColumnType::Varchar),
        ],
    });
    let path = std::env::temp_dir().join("isbd_roundtrip_test.isbd");
    let path = path.to_str().unwrap();

    let mut batches = Vec::new();
    for size in [1024, 1024, 100] {
        let mut batch = Batch::new(schema.clone());
        for i in 0..size {
            let x = match i % 5 {
                0 => i64::MIN,
                1 => i64::MAX,
                _ => i as i64 * -37,
            };
            if let Column::Int64(v) = &mut batch.columns[0] {
                v.push(x);
            }
            if let Column::Bool(b) = &mut batch.columns[1] {
                b.push(i % 3 == 0);
            }
            if let Column::Varchar(s) = &mut batch.columns[2] {
                s.push(if i % 7 == 0 { "".as_bytes() } else { "zażółć gęślą jaźń".as_bytes() });
            }
        }
        batches.push(batch);
    }

    let mut w = Writer::create(path, (*schema).clone()).unwrap();
    for b in &batches {
        w.write_batch(b).unwrap();
    }
    w.finish().unwrap();

    let r = Reader::open(path).unwrap();
    assert_eq!(*r.schema, *schema);
    assert_eq!(r.num_rows(), 2148);
    let read: Vec<Batch> = r.map(|b| b.unwrap()).collect();
    assert_eq!(read, batches);
    std::fs::remove_file(path).unwrap();
}
