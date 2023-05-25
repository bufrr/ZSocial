#![allow(dead_code, unused)]

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use nokv::{lmdb::Transaction, scanner::*, Error};
use nokv_bench::*;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Key {
    // k: &'txn [u8],
    // v: &'txn [u8],
    time: u64,
    id: u64,
}

impl Key {
    fn encode(kind: u64, time: u64) -> Vec<u8> {
        [&kind.to_be_bytes()[..], &time.to_be_bytes()[..]].concat()
    }

    fn from(k: &[u8], v: &[u8]) -> Self {
        Self {
            time: u64::from_be_bytes(k[8..16].try_into().unwrap()),
            id: u64::from_be_bytes(v[0..8].try_into().unwrap()),
            // v,
        }
    }

    fn uid(&self) -> u64 {
        self.id
    }
}

impl TimeKey for Key {
    fn time(&self) -> u64 {
        self.time
    }

    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time()
            .cmp(&other.time())
            .then_with(|| self.uid().cmp(&other.uid()))
    }

    fn change_time(&self, key: &[u8], time: u64) -> Vec<u8> {
        [&key[0..8], &time.to_be_bytes()[..]].concat()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error(transparent)]
    Db(#[from] Error),
}

fn bench_scanner1(c: &mut Criterion) {
    bench_scanner(c, 1_000_000, 10_000);
}

fn bench_scanner(c: &mut Criterion, init_len: usize, chunk_size: usize) {
    let num_str = fmt_num(init_len as f64);

    println!("Generate initial data {}", num_str);
    let now = Instant::now();
    let initial = gen_pairs(16, 8, init_len);
    println!("Generated in {:?}", now.elapsed());
    let initial_chunks = chunk_vec(&initial, chunk_size);

    let mut group = c.benchmark_group(format!("scanner-{}-{}", num_str, chunk_size));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(50);
    group.warm_up_time(Duration::from_secs(1));
    group.throughput(Throughput::Elements(init_len as u64));
    // group.throughput(Throughput::Elements(1));
    let dir = tempfile::Builder::new()
        .prefix("nokv-bench-scanner")
        .tempdir()
        .unwrap();
    let db = nokv::lmdb::Db::open_with(
        dir.path(),
        Some(30),
        Some(1_000),
        Some(1_000_000_000_000),
        0,
    )
    .unwrap();
    let tree = db.open_tree(Some("t1"), 0).unwrap();

    {
        println!("lmdb: Put initial data batch {}", chunk_size);
        let now = Instant::now();
        for chunk in initial_chunks.iter() {
            let mut writer = db.writer().unwrap();
            for (k, v) in chunk {
                writer.put(&tree, k, v).unwrap();
            }
            writer.commit().unwrap();
        }
        println!(
            "put in {:?} {:?}",
            now.elapsed(),
            fmt_per_sec(init_len, &now.elapsed())
        );
    }

    // {
    //     let reader = db.reader().unwrap();
    //     group.bench_function("count", |b| {
    //         b.iter(|| {
    //             let mut iter = reader.iter(&tree);
    //             black_box(&iter);
    //             let mut total = 0;
    //             while let Some(kv) = iter.next() {
    //                 let kv = kv.unwrap();
    //                 total += 1;
    //             }
    //             black_box(total);
    //         })
    //     });
    // }

    {
        let reader = db.reader().unwrap();
        group.bench_function("scanner-count", |b| {
            b.iter(|| {
                let iter = reader.iter(&tree);
                let prefix = vec![];
                let mut scanner = Scanner::<_, MyError>::new(
                    iter,
                    prefix.clone(),
                    prefix.clone(),
                    false,
                    None,
                    None,
                    Box::new(|s, (k, v)| Ok(MatchResult::Found(Key::from(k, v)))),
                );
                let mut total = 0;
                while let Some(kv) = scanner.next() {
                    let kv = kv.unwrap();
                    // black_box(kv);
                    total += 1;
                }
                black_box(total);
            });
        });
    }

    {
        let reader = db.reader().unwrap();

        group.bench_function("group-count", |b| {
            b.iter(|| {
                let iter = reader.iter(&tree);
                let mut group = Group::new(false, false);
                let prefix = vec![];
                let scanner = Scanner::<_, MyError>::new(
                    iter,
                    prefix.clone(),
                    prefix.clone(),
                    false,
                    None,
                    None,
                    Box::new(|s, (k, v)| Ok(MatchResult::Found(Key::from(k, v)))),
                );
                group.add(0, scanner).unwrap();
                let mut _total = 0;
                while let Some(kv) = group.next() {
                    let kv = kv.unwrap();
                    black_box(kv);
                    _total += 1;
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_scanner1);
criterion_main!(benches);
