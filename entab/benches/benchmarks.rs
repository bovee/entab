use std::fs::File;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use entab::buffer::ReadBuffer;
use entab::readers::chemstation::ChemstationMsReaderBuilder;
use entab::readers::fasta::FastaReaderBuilder;
use entab::readers::fastq::FastqReaderBuilder;
use entab::readers::ReaderBuilder;

fn benchmark_readers(c: &mut Criterion) {
    c.bench_function("chemstation reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let builder = ChemstationMsReaderBuilder::default();
            let mut reader = builder.to_reader(rb).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    c.bench_function("fasta reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/sequence.fasta").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let builder = FastaReaderBuilder::default();
            let mut reader = builder.to_reader(rb).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    c.bench_function("fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let builder = FastqReaderBuilder::default();
            let mut reader = builder.to_reader(rb).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });
}

criterion_group!(benches, benchmark_readers);
criterion_main!(benches);
