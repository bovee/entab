use std::fs::File;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use entab::compression::decompress;
use entab::parsers::agilent::chemstation::ChemstationMsReader;
use entab::parsers::fasta::FastaReader;
use entab::parsers::fastq::{FastqReader, FastqRecord, FastqState};
use entab::parsers::png::PngReader;
use entab::parsers::sam::BamReader;
use entab::readers::{get_reader, init_state};

fn benchmark_raw_readers(c: &mut Criterion) {
    let mut raw_readers = c.benchmark_group("raw readers");
    raw_readers.significance_level(0.01).sample_size(500);

    raw_readers.bench_function("chemstation reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS").unwrap();
            let mut reader = ChemstationMsReader::new(f, None).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fasta reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/sequence.fasta").unwrap();
            let mut reader = FastaReader::new(f, None).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let mut reader = FastqReader::new(f, None).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fastq [unsafe] reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let (mut rb, mut state) = init_state::<FastqState, _, _>(f, None).unwrap();
            let mut record = FastqRecord::default();
            while unsafe { rb.next_into(&mut state, &mut record).unwrap() } {
                let FastqRecord { sequence, .. } = &record;
                black_box(sequence);
            }
        })
    });

    raw_readers.bench_function("png reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/bmp_24.png").unwrap();
            let mut reader = PngReader::new(f, None).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("bam reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.bam").unwrap();
            let (rb, _) = decompress(f).unwrap();
            let mut reader = BamReader::new(rb, None).unwrap();
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
            let (mut reader, _) = get_reader(f, Some("chemstation_ms"), None).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("generic fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let (mut reader, _) = get_reader(f, Some("fastq"), None).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("flow reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs").unwrap();
            let (mut reader, _) = get_reader(f, Some("flow"), None).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("png reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/bmp_24.png").unwrap();
            let (mut reader, _) = get_reader(f, Some("png"), None).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });
}

criterion_group!(benches, benchmark_raw_readers, benchmark_generic_readers);
criterion_main!(benches);
