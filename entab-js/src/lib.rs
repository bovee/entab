mod utils;

use std::collections::HashMap;
use std::io::{Cursor, Read};

use entab_base::buffer::ReadBuffer;
use entab_base::compression::decompress;
use entab_base::readers::{get_builder, RecordReader};
use entab_base::record::Record as EtRecord;
use entab_base::utils::error::EtError;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[derive(Serialize)]
pub struct NextRecord {
    value: Option<Record>,
    done: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record {
    Mz {
        time: f64,
        mz: f64,
        intensity: f64,
    },
    Sam {
        query_name: String,
        flag: u16,
        ref_name: String,
        pos: Option<u64>,
        mapq: Option<u8>,
        cigar: String,
        rnext: String,
        pnext: Option<u32>,
        tlen: i32,
        seq: String,
        qual: String,
        extra: String,
    },
    Sequence {
        id: String,
        sequence: String,
        quality: Option<String>,
    },
    Tsv(HashMap<String, String>),
}

fn to_owned_rec(rec: EtRecord) -> Result<Record, EtError> {
    Ok(match rec {
        EtRecord::Mz {
            time,
            mz,
            intensity,
        } => Record::Mz {
            time,
            mz,
            intensity,
        },
        EtRecord::Sam {
            query_name,
            flag,
            ref_name,
            pos,
            mapq,
            cigar,
            rnext,
            pnext,
            tlen,
            seq,
            qual,
            extra,
        } => Record::Sam {
            query_name: query_name.to_string(),
            flag,
            ref_name: ref_name.to_string(),
            pos,
            mapq,
            cigar: String::from_utf8(cigar.to_vec())?,
            rnext: rnext.to_string(),
            pnext,
            tlen,
            seq: String::from_utf8(seq.to_vec())?,
            qual: String::from_utf8(qual.to_vec())?,
            extra: String::from_utf8(extra.to_vec())?,
        },
        EtRecord::Sequence {
            id,
            sequence,
            quality,
        } => {
            let quality = if let Some(q) = quality {
                Some(String::from_utf8(q.to_vec())?)
            } else {
                None
            };
            Record::Sequence {
                id: id.to_string(),
                sequence: String::from_utf8(sequence.to_vec())?,
                quality,
            }
        }
        EtRecord::Tsv(recs, headers) => {
            let map = headers
                .iter()
                .zip(recs.iter())
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect();
            Record::Tsv(map)
        }
    })
}

#[wasm_bindgen]
pub struct Reader {
    parser: String,
    reader: Box<dyn RecordReader>,
}

fn to_js(err: EtError) -> JsValue {
    err.to_string().into()
}

#[wasm_bindgen]
impl Reader {
    #[wasm_bindgen(constructor)]
    pub fn new(data: Box<[u8]>, parser: Option<String>) -> Result<Reader, JsValue> {
        utils::set_panic_hook();

        let stream: Box<dyn Read> = Box::new(Cursor::new(data));

        let (reader, filetype, _) = decompress(stream).map_err(to_js)?;
        let buffer = ReadBuffer::new(reader).map_err(to_js)?;

        let parser_name = parser.unwrap_or_else(|| filetype.to_parser_name().to_string());
        let reader = if let Some(builder) = get_builder(&parser_name) {
            builder.to_reader(buffer).map_err(to_js)?
        } else {
            return Err(JsValue::from_str(
                "No reader could be found for this file type",
            ));
        };
        Ok(Reader {
            parser: parser_name.to_string(),
            reader,
        })
    }

    #[wasm_bindgen(getter)]
    pub fn parser(&self) -> String {
        self.parser.clone()
    }

    // FIXME: it'd be nice to implement iterable
    // #[wasm_bindgen(js_name = "@@iterable")]
    // pub fn iterable(&self) -> JsValue {
    //     self
    // }

    #[wasm_bindgen]
    pub fn next(&mut self) -> Result<JsValue, JsValue> {
        if let Some(value) = self.reader.next().map_err(to_js)? {
            JsValue::from_serde(&NextRecord {
                value: Some(to_owned_rec(value).map_err(to_js)?),
                done: false,
            })
            .map_err(|_| JsValue::from_str("Error translating record"))
        } else {
            JsValue::from_serde(&NextRecord {
                value: None,
                done: false,
            })
            .map_err(|_| JsValue::from_str("Error translating record"))
        }
    }
}
