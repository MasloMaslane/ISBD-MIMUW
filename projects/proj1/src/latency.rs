use std::env;
use std::thread;
use std::fs::File;
use std::sync::Arc;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicU64, Ordering};
use rand::{rng, Rng, RngExt, SeedableRng};
use rand::rngs::StdRng;

mod hash;
use hash::sum_block_hash;

pub struct RandomOrderIterator {
    elements: Vec<u64>,
    rng: Box<dyn Rng>
}

impl RandomOrderIterator {
    pub fn with_seed(size: u64, seed: u64) -> Self {
        Self { elements: (0 as u64..size).collect() , rng: Box::new(StdRng::seed_from_u64(seed)), }
    }

    pub fn new(size: u64) -> Self {
        Self {
            elements: (0..size).collect(),
            rng: Box::new(rng()),
        }
    }
}

impl Iterator for RandomOrderIterator {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.elements.is_empty() {
            None
        } else {
            let rand_idx = self.rng.random_range(0..self.elements.len());
            Some(self.elements.swap_remove(rand_idx))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.elements.len(), Some(self.elements.len()))
    }
}

const RAND_SEED: u64 = 1234;

fn main() {

    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        println!("Usage: cargo run --release --bin latency filepath threads block_size");
        return;
    }
    let num_threads: usize = args[2].parse().unwrap();
    let block_size: u64 = args[3].parse().unwrap();

    let file_res = File::open(args[1].clone());
    if file_res.is_err() {
        eprintln!("File not found: {}", args[1]);
        return;
    }
    let file = Arc::new(file_res.unwrap());

    let metadata = file.metadata();
    if metadata.is_err() {
        eprintln!("Error opening file: {}", args[1]);
        return;
    }
    let total_size = metadata.unwrap().len();
    let num_blocks = (total_size + block_size - 1) / block_size;
    let random_iter = RandomOrderIterator::with_seed(num_blocks, RAND_SEED);

    let mut result = vec![0u8; 16];
    let block_idxs = Arc::new(random_iter.collect::<Vec<u64>>());
    let (tx, rx) = std::sync::mpsc::channel();
    let read_ns = Arc::new(AtomicU64::new(0));

    let handles = (0..num_threads).map(|thread_idx| {
        let file_clone = Arc::clone(&file);
        let block_idxs_clone = Arc::clone(&block_idxs);
        let tx_clone = tx.clone();
        let read_ns_clone = Arc::clone(&read_ns);

        thread::spawn(move || {
            for block_idx in block_idxs_clone.iter().skip(thread_idx).step_by(num_threads) {
                let offset = block_idx * block_size;
                let bytes_to_read = std::cmp::min(block_size, total_size - offset) as usize;
                let mut buffer = vec![0u8; bytes_to_read];

                // read
                let start = std::time::Instant::now();
                let err = file_clone.read_exact_at(&mut buffer, offset);
                let elapsed = start.elapsed();
                if err.is_err() {
                    eprintln!("Error reading file: {}", err.unwrap_err());
                    std::process::exit(1);
                }
                read_ns_clone.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);

                tx_clone.send(buffer).unwrap();
            }
        })
    }).collect::<Vec<_>>();
    drop(tx);

    for msg in rx {
        sum_block_hash(&mut result, &md5::compute(&msg));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    eprintln!("avg_read_us={}", read_ns.load(Ordering::Relaxed) as f64 / num_blocks as f64 / 1000.0);
    print!("{} 0x", args[1].clone());
    for byte in &result {
        print!("{:02x}", byte);
    }
    println!("");

}
