use std::fs::File;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use entab::buffer::ReadBuffer;
use entab::readers::chemstation::ChemstationMsReader;
use entab::readers::fasta::FastaReader;
use entab::readers::fastq::FastqReader;
use entab::readers::get_reader;

fn benchmark_readers(c: &mut Criterion) {
    c.bench_function("chemstation reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = ChemstationMsReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    c.bench_function("fasta reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/sequence.fasta").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = FastaReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    c.bench_function("fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = FastqReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    c.bench_function("generic fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = get_reader("fastq", rb).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });
}

criterion_group!(benches, benchmark_readers);
criterion_main!(benches);
