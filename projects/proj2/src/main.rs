use std::env;
use std::sync::Arc;

use rand::RngExt;
use rand::seq::IndexedRandom;

use proj2::{BATCH_SIZE, Batch, Column, ColumnType, Reader, Schema, Writer};

const NAMES: [&str; 8] = ["Anna", "Paweł", "Łukasz", "Zofia", "Michał", "Ewa", "Jan", "Małgorzata"];
const CHARS: &str = "abcdefghijklmnopqrstuvwxyz ABCXYZ0123456789ąęółżźćń";

fn generate(path: &str, rows: usize) {
    let schema = Arc::new(Schema {
        columns: vec![
            ("id".to_string(), ColumnType::Int64),
            ("active".to_string(), ColumnType::Bool),
            ("name".to_string(), ColumnType::Varchar),
            ("comment".to_string(), ColumnType::Varchar),
        ],
    });
    let chars: Vec<char> = CHARS.chars().collect();
    let mut rng = rand::rng();
    let mut writer = Writer::create(path, (*schema).clone()).expect("cannot create file");

    let mut done = 0;
    let mut id: i64 = -500;
    while done < rows {
        let n = BATCH_SIZE.min(rows - done);
        let mut batch = Batch::new(schema.clone());
        for _ in 0..n {
            id += rng.random_range(0..10);
            let mut comment = String::new();
            for _ in 0..rng.random_range(0..40) {
                comment.push(*chars.choose(&mut rng).unwrap());
            }
            let name = NAMES.choose(&mut rng).unwrap();

            if let Column::Int64(v) = &mut batch.columns[0] {
                v.push(id);
            }
            if let Column::Bool(b) = &mut batch.columns[1] {
                b.push(rng.random_bool(0.25));
            }
            if let Column::Varchar(s) = &mut batch.columns[2] {
                s.push(name.as_bytes());
            }
            if let Column::Varchar(s) = &mut batch.columns[3] {
                s.push(comment.as_bytes());
            }
        }
        writer.write_batch(&batch).expect("write failed");
        done += n;
    }
    writer.finish().expect("write failed");
    println!("wrote {} rows to {}", rows, path);
}

fn entropy(p: f64) -> f64 {
    if p <= 0.0 || p >= 1.0 {
        return 0.0;
    }
    -p * p.log2() - (1.0 - p) * (1.0 - p).log2()
}

fn stats(path: &str) {
    let reader = match Reader::open(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error opening {}: {}", path, e);
            return;
        }
    };
    let schema = reader.schema.clone();
    let rows = reader.num_rows();
    let ncols = schema.columns.len();
    let mut sums = vec![0i128; ncols]; // sum / true count / ascii char count

    for batch in reader {
        let batch = batch.expect("read failed");
        for i in 0..ncols {
            match &batch.columns[i] {
                Column::Int64(v) => sums[i] += v.iter().map(|&x| x as i128).sum::<i128>(),
                Column::Bool(b) => sums[i] += b.count_ones() as i128,
                Column::Varchar(s) => sums[i] += s.data.iter().filter(|c| c.is_ascii()).count() as i128,
            }
        }
    }

    println!("rows: {}", rows);
    for (i, (name, t)) in schema.columns.iter().enumerate() {
        match t {
            ColumnType::Int64 => println!("{} {}: avg = {}", name, t.name(), sums[i] as f64 / rows as f64),
            ColumnType::Bool => println!("{} {}: entropy = {:.6}", name, t.name(), entropy(sums[i] as f64 / rows as f64)),
            ColumnType::Varchar => println!("{} {}: ascii chars = {}", name, t.name(), sums[i]),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 4 && args[1] == "gen" {
        generate(&args[2], args[3].parse().expect("rows must be a number"));
    } else if args.len() == 3 && args[1] == "stats" {
        stats(&args[2]);
    } else {
        println!("Usage: isbd gen <file> <rows>");
        println!("       isbd stats <file>");
    }
}
