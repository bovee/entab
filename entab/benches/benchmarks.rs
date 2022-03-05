use std::fs::File;
use std::sync::Mutex;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use entab::buffer::ReadBuffer;
use entab::chunk::init_state;
use entab::compression::decompress;
use entab::filetype::FileType;
use entab::readers::agilent::chemstation::ChemstationMsReader;
use entab::readers::fasta::FastaReader;
use entab::readers::fastq::{FastqReader, FastqRecord, FastqState};
use entab::readers::get_reader;
use entab::readers::png::PngReader;
use entab::readers::sam::BamReader;

fn benchmark_raw_readers(c: &mut Criterion) {
    let mut raw_readers = c.benchmark_group("raw readers");
    raw_readers.significance_level(0.01).sample_size(500);

    raw_readers.bench_function("chemstation reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS").unwrap();
            let mut reader = ChemstationMsReader::new(f, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fasta reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/sequence.fasta").unwrap();
            let mut reader = FastaReader::new(f, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let mut reader = FastqReader::new(f, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("fastq [chunk] reader", |b| {
        b.iter(|| {
            let f = File::open("./tests/data/test.fastq").unwrap();
            let (mut rb, mut state) = init_state::<FastqState, _, _>(f, None).unwrap();
            while let Some(FastqRecord { sequence, .. }) = rb.next(&mut state).unwrap() {
                black_box(sequence);
            }
        })
    });

    raw_readers.bench_function("fastq [chunk - threaded] reader", |b| {
        b.iter(|| {
            let f = File::open("./tests/data/test.fastq").unwrap();
            let (mut rb, mut state) = init_state::<FastqState, _, _>(f, None).unwrap();
            while let Some((slice, mut chunk)) = rb.next_chunk().unwrap() {
                let mut_state = Mutex::new(&mut state);
                let chunk = rayon::scope(|s| {
                    while let Some(FastqRecord { sequence, .. }) =
                        chunk.next(slice, &mut_state).map_err(|e| e.to_string())?
                    {
                        s.spawn(move |_| {
                            black_box(sequence);
                        });
                    }
                    Ok::<_, String>(chunk)
                })
                .unwrap();
                rb.update_from_chunk(chunk);
            }
        })
    });

    raw_readers.bench_function("png reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/bmp_24.png").unwrap();
            let mut reader = PngReader::new(f, ()).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        })
    });

    raw_readers.bench_function("bam reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.bam").unwrap();
            let (stream, _, _) = decompress(Box::new(f)).unwrap();
            let rb = ReadBuffer::from_reader(stream, None).unwrap();
            let mut reader = BamReader::new(rb, ()).unwrap();
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
            let mut reader = get_reader(FileType::AgilentChemstationMs, f).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("generic fastq reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/test.fastq").unwrap();
            let mut reader = get_reader(FileType::Fastq, f).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("flow reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs").unwrap();
            let mut reader = get_reader(FileType::Facs, f).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });

    generic_readers.bench_function("png reader", |b| {
        b.iter(|| {
            let f = File::open("tests/data/bmp_24.png").unwrap();
            let mut reader = get_reader(FileType::Png, f).unwrap();
            while let Some(record) = reader.next_record().unwrap() {
                black_box(record);
            }
        })
    });
}

criterion_group!(benches, benchmark_raw_readers, benchmark_generic_readers);
criterion_main!(benches);
