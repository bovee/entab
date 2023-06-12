use std::fs::File;
use std::io::BufReader;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use bio::io::fasta as bio_fasta;
use bio::io::fastq as bio_fastq;

use entab::compression::decompress;
use entab::parsers::fasta::{FastaReader, FastaRecord};
use entab::parsers::fastq::{FastqReader, FastqRecord};
use entab::readers::init_state;
use entab::parsers::sam::{BamReader, BamRecord, SamReader, SamRecord};

use needletail::parse_fastx_file;

use noodles::bam as noodles_bam;
use noodles::fasta as noodles_fasta;
use noodles::fastq as noodles_fastq;
use noodles::sam as noodles_sam;

use rust_htslib::{bam, bam::Read};

use seq_io::{fasta as seqio_fasta, fastq as seqio_fastq};

mod fasta;

const BAM_PATH: &str = "../entab/tests/data/test.bam";
const FASTA_PATH: &str = "../entab/tests/data/sequence.fasta";
const FASTQ_PATH: &str = "../entab/tests/data/test.fastq";
const SAM_PATH: &str = "../entab/tests/data/test.sam";

fn benchmark_bam(c: &mut Criterion) {
    let mut bam = c.benchmark_group("bam");
    bam.significance_level(0.01).sample_size(100);
    bam.warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(20));

    bam.bench_function("entab", |b| {
        b.iter(|| {
            let file = File::open(BAM_PATH).unwrap();
            let (rb, _) = decompress(file).unwrap();
            let mut reader = BamReader::new(rb, None).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        });
    });

    bam.bench_function("entab [unsafe]", |b| {
        b.iter(|| {
            let file = File::open(BAM_PATH).unwrap();
            let (stream, _) = decompress(file).unwrap();
            let (mut rb, mut state) = init_state(stream, None).unwrap();
            let mut record = BamRecord::default();
            while unsafe { rb.next_into(&mut state, &mut record).unwrap() } {
                let BamRecord { cigar, .. } = &record;
                black_box(cigar);
            }
        });
    });

    bam.bench_function("htslib", |b| {
        b.iter(|| {
            let mut reader = bam::Reader::from_path(BAM_PATH).unwrap();
            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    bam.bench_function("noodles", |b| {
        b.iter(|| {
            let mut reader = File::open(BAM_PATH).map(noodles_bam::Reader::new).unwrap();
            reader.read_header().unwrap();
            reader.read_reference_sequences().unwrap();

            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });
}

fn benchmark_fasta(c: &mut Criterion) {
    let mut fasta = c.benchmark_group("fasta");
    fasta.significance_level(0.01).sample_size(100);
    fasta
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(20));

    // Part of why this benchmark appears to be so fast is that we're embedding the
    // test file directly in here so it's not really an apples-to-apples comparison
    // with the other readers. This was using memmap to read in the file initially,
    // but that consumes a fair amount of startup overhead (which needletail also
    // has) so ideally we should be testing against a _very_ large fasta here like
    // GRCh38 or something like that.
    fasta.bench_function("hyper optimized", |b| {
        let mut rb: &[u8] = include_bytes!("../../entab/tests/data/sequence.fasta");
        b.iter(|| {
            fasta::read_fasta(rb, |id, seq| {
                black_box(id);
                black_box(seq);
            }).unwrap();
        });
    });

    fasta.bench_function("entab", |b| {
        b.iter(|| {
            let file = File::open(FASTA_PATH).unwrap();
            let mut reader = FastaReader::new(file, None).unwrap();
            while let Some(FastaRecord { sequence, .. }) = reader.next().unwrap() {
                black_box(sequence);
            }
        });
    });

    fasta.bench_function("entab [unsafe]", |b| {
        b.iter(|| {
            let file = File::open(FASTA_PATH).unwrap();
            let (mut rb, mut state) = init_state(file, None).unwrap();
            let mut record = FastaRecord::default();
            while unsafe { rb.next_into(&mut state, &mut record).unwrap() } {
                let FastaRecord { sequence, .. } = &record;
                black_box(sequence);
            }
        });
    });

    fasta.bench_function("needletail", |b| {
        b.iter(|| {
            let mut reader = parse_fastx_file(FASTQ_PATH).unwrap();
            while let Some(result) = reader.next() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    fasta.bench_function("noodles", |b| {
        b.iter(|| {
            let mut reader = File::open(FASTA_PATH)
                .map(BufReader::new)
                .map(noodles_fasta::Reader::new)
                .unwrap();

            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    fasta.bench_function("rust-bio", |b| {
        b.iter(|| {
            let reader = bio_fasta::Reader::from_file(FASTA_PATH).unwrap();

            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    fasta.bench_function("seq_io", |b| {
        b.iter(|| {
            let mut reader = seqio_fasta::Reader::from_path(FASTA_PATH).unwrap();

            while let Some(record) = reader.next() {
                let record = record.unwrap();
                black_box(record);
            }
        });
    });
}

fn benchmark_fastq(c: &mut Criterion) {
    let mut fastq = c.benchmark_group("fastq");
    fastq.significance_level(0.01).sample_size(100);
    fastq
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(20));

    fastq.bench_function("entab", |b| {
        b.iter(|| {
            let file = File::open(FASTQ_PATH).unwrap();
            let mut reader = FastqReader::new(file, None).unwrap();
            while let Some(FastqRecord { sequence, .. }) = reader.next().unwrap() {
                black_box(sequence);
            }
        });
    });

    fastq.bench_function("entab [unsafe]", |b| {
        b.iter(|| {
            let file = File::open(FASTQ_PATH).unwrap();
            let (mut rb, mut state) = init_state(file, None).unwrap();
            let mut record = FastqRecord::default();
            while unsafe { rb.next_into(&mut state, &mut record).unwrap() } {
                let FastqRecord { sequence, .. } = &record;
                black_box(sequence);
            }
        });
    });

    fastq.bench_function("needletail", |b| {
        b.iter(|| {
            let mut reader = parse_fastx_file(FASTQ_PATH).unwrap();
            while let Some(result) = reader.next() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    fastq.bench_function("noodles", |b| {
        b.iter(|| {
            let mut reader = File::open(FASTQ_PATH)
                .map(BufReader::new)
                .map(noodles_fastq::Reader::new)
                .unwrap();

            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    fastq.bench_function("rust-bio", |b| {
        b.iter(|| {
            let reader = bio_fastq::Reader::from_file(FASTQ_PATH).unwrap();

            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    fastq.bench_function("seq_io", |b| {
        b.iter(|| {
            let mut reader = seqio_fastq::Reader::from_path(FASTQ_PATH).unwrap();

            while let Some(record) = reader.next() {
                let record = record.unwrap();
                black_box(record);
            }
        });
    });
}

fn benchmark_sam(c: &mut Criterion) {
    let mut sam = c.benchmark_group("sam");
    sam.significance_level(0.01).sample_size(100);
    sam.warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(20));

    sam.bench_function("entab", |b| {
        b.iter(|| {
            let file = File::open(SAM_PATH).unwrap();
            let mut reader = SamReader::new(file, None).unwrap();
            while let Some(record) = reader.next().unwrap() {
                black_box(record);
            }
        });
    });

    sam.bench_function("entab [unsafe]", |b| {
        b.iter(|| {
            let file = File::open(SAM_PATH).unwrap();
            let (mut rb, mut state) = init_state(file, None).unwrap();
            let mut record = SamRecord::default();
            while unsafe { rb.next_into(&mut state, &mut record).unwrap() } {
                black_box(&record);
            }
        });
    });

    sam.bench_function("htslib", |b| {
        b.iter(|| {
            let mut reader = bam::Reader::from_path(SAM_PATH).unwrap();
            for result in reader.records() {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });

    sam.bench_function("noodles", |b| {
        b.iter(|| {
            let mut reader = File::open(SAM_PATH)
                .map(BufReader::new)
                .map(noodles_sam::Reader::new)
                .unwrap();
            let header = reader.read_header().unwrap().parse().unwrap();

            for result in reader.records(&header) {
                let record = result.unwrap();
                black_box(record);
            }
        });
    });
}

criterion_group!(
    benches,
    benchmark_bam,
    benchmark_fasta,
    benchmark_fastq,
    benchmark_sam
);
criterion_main!(benches);
