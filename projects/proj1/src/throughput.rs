use std::env;
use std::fs::File;
use std::os::unix::fs::FileExt;

use memmap2::{Advice, Mmap};

mod hash;
use hash::sum_block_hash;

fn main() {

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 && args.len() != 4 {
        println!("Usage: cargo run --release --bin throughput filepath block_size [read|mmap]");
        return;
    }
    let block_size: u64 = args[2].parse().unwrap();
    let use_mmap = args.get(3).map(|s| s.as_str()) != Some("read");

    let file_res = File::open(args[1].clone());
    if file_res.is_err() {
        eprintln!("File not found: {}", args[1]);
        return;
    }
    let file = file_res.unwrap();

    let metadata = file.metadata();
    if metadata.is_err() {
        eprintln!("Error opening file: {}", args[1]);
        return;
    }
    let total_size = metadata.unwrap().len();
    let num_blocks = (total_size + block_size - 1) / block_size;

    let start = std::time::Instant::now();
    let mut result = vec![0u8; 16];
    if use_mmap {
        if total_size > 0 {
            let map = unsafe { Mmap::map(&file).unwrap() };
            map.advise(Advice::Sequential).unwrap();
            for block in map.chunks(block_size as usize) {
                sum_block_hash(&mut result, &md5::compute(block));
            }
        }
    } else {
        let mut buffer = vec![0u8; block_size as usize];
        for block_idx in 0..num_blocks
        {
            let offset = block_idx * block_size;
            let bytes_to_read = std::cmp::min(block_size, total_size - offset) as usize;

            // read
            let target_buf = &mut buffer[..bytes_to_read];
            let err = file.read_exact_at(target_buf, offset);
            if err.is_err() {
                eprintln!("Error reading file: {}", err.unwrap_err());
                return;
            }

            // compute
            let block_hash = md5::compute(target_buf);
            sum_block_hash(&mut result, &block_hash);
        }
    }
    eprintln!("elapsed_s={}", start.elapsed().as_secs_f64());

    print!("{} 0x", args[1].clone());
    for byte in &result {
        print!("{:02x}", byte);
    }
    println!("");

}
