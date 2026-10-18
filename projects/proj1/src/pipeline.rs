use std::env;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::Mutex;
use std::sync::mpsc::sync_channel;
use std::thread;

mod hash;
use hash::sum_block_hash;
use hash::stretch_md5;

fn main() {

    let args: Vec<String> = env::args().collect();
    if args.len() < 4 || args.len() > 8 {
        println!("Usage: cargo run --release --bin pipeline filepath block_size k [seq|pipe] [readers] [hashers] [channel_size]");
        return;
    }
    let block_size: u64 = args[2].parse().unwrap();
    let k: u64 = args[3].parse().unwrap();
    let pipe = args.get(4).map(|s| s.as_str()) != Some("seq");
    let readers: u64 = args.get(5).map_or(1, |s| s.parse().unwrap());
    let hashers: usize = args.get(6).map_or(1, |s| s.parse().unwrap());
    let channel_size: usize = args.get(7).map_or(4, |s| s.parse().unwrap());

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

    let read_block = |block_idx: u64, buffer: &mut [u8]| -> usize {
        let offset = block_idx * block_size;
        let bytes_to_read = std::cmp::min(block_size, total_size - offset) as usize;
        if let Err(err) = file.read_exact_at(&mut buffer[..bytes_to_read], offset) {
            eprintln!("Error reading file: {}", err);
            std::process::exit(1);
        }
        bytes_to_read
    };

    let mut result = vec![0u8; 16];
    if !pipe {
        let mut buffer = vec![0u8; block_size as usize];
        for block_idx in 0..num_blocks {
            let len = read_block(block_idx, &mut buffer);
            sum_block_hash(&mut result, &stretch_md5(&buffer[..len], k));
        }
    } else {
        let (tx, rx) = sync_channel::<Vec<u8>>(channel_size);
        let rx = Mutex::new(rx);
        thread::scope(|s| {
            for r in 0..readers {
                let tx = tx.clone();
                let read_block = &read_block;
                s.spawn(move || {
                    for block_idx in (r..num_blocks).step_by(readers as usize) {
                        let mut buffer = vec![0u8; block_size as usize];
                        let len = read_block(block_idx, &mut buffer);
                        buffer.truncate(len);
                        tx.send(buffer).unwrap();
                    }
                });
            }
            drop(tx);

            let handles: Vec<_> = (0..hashers).map(|_| {
                let rx = &rx;
                s.spawn(move || {
                    let mut partial = vec![0u8; 16];
                    loop {
                        let msg = rx.lock().unwrap().recv();
                        match msg {
                            Ok(block) => sum_block_hash(&mut partial, &stretch_md5(&block, k)),
                            Err(_) => break,
                        }
                    }
                    partial
                })
            }).collect();
            for h in handles {
                sum_block_hash(&mut result, h.join().unwrap()[..].try_into().unwrap());
            }
        });
    }

    print!("{} 0x", args[1].clone());
    for byte in &result {
        print!("{:02x}", byte);
    }
    println!("");

}
