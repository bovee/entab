use std::fs::File;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use entab::buffer::ReadBuffer;
use entab::readers::chemstation::ChemstationMsReader;
use entab::readers::fasta::FastaReader;
use entab::readers::fastq::FastqReader;
use entab::readers::get_reader;
use entab::readers::png::PngReader;

fn benchmark_raw_readers(c: &mut Criterion) {
    let mut raw_readers = c.benchmark_group("raw readers");
    raw_readers.significance_level(0.01).sample_size(500);

    raw_readers.bench_function("chemstation reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = ChemstationMsReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fasta reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/sequence.fasta").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = FastaReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = FastqReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("png reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/bmp_24.png").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = PngReader::new(rb, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });
}

fn benchmark_generic_readers(c: &mut Criterion) {
    let mut generic_readers = c.benchmark_group("generic readers");
    generic_readers.significance_level(0.01).sample_size(500);
    generic_readers.bench_function("generic chemstation reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = get_reader("chemstation_ms", rb).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("generic fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = get_reader("fastq", rb).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("flow reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = get_reader("fcs", rb).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("png reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/bmp_24.png").unwrap();
            let rb = ReadBuffer::new(Box::new(&f)).unwrap();
            let mut reader = get_reader("png", rb).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });
}

criterion_group!(benches, benchmark_raw_readers, benchmark_generic_readers);
criterion_main!(benches);
