mod utils;

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use entab_base::compression::decompress;
use entab_base::error::EtError;
use entab_base::filetype::FileType;
use entab_base::readers::{get_reader, RecordReader};
use entab_base::record::Value;
use js_sys::Array;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[derive(Serialize)]
pub struct NextRecord<'v> {
    value: Option<BTreeMap<&'v str, Value<'v>>>,
    done: bool,
}

#[wasm_bindgen]
pub struct Reader {
    parser: String,
    headers: Vec<String>,
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
        if data.is_empty() {
            return Err(JsValue::from_str("Data is empty or of the wrong type."));
        }
        let stream: Box<dyn Read> = Box::new(Cursor::new(data));

        let (reader, filetype, _) = decompress(stream).map_err(to_js)?;

        let filetype = parser
            .map(|p| FileType::from_parser_name(&p))
            .unwrap_or_else(|| filetype);
        let reader = get_reader(filetype, reader).map_err(to_js)?;
        let headers = reader.headers();
        Ok(Reader {
            parser: filetype.to_parser_name(),
            headers,
            reader,
        })
    }

    // TODO: it'd be nice to implement @@iterator somehow in here too

    #[wasm_bindgen(getter)]
    pub fn parser(&self) -> String {
        self.parser.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn headers(&self) -> JsValue {
        let array = Array::new();
        for item in &self.headers {
            array.push(&item.into());
        }
        array.into()
    }

    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> Result<JsValue, JsValue> {
        JsValue::from_serde(&self.reader.metadata())
            .map_err(|_| JsValue::from_str("Error translating metadata"))
    }

    #[allow(clippy::should_implement_trait)]
    #[wasm_bindgen]
    pub fn next(&mut self) -> Result<JsValue, JsValue> {
        if let Some(value) = self.reader.next_record().map_err(to_js)? {
            let obj: BTreeMap<&str, Value> = self
                .headers
                .iter()
                .map(|i| i.as_ref())
                .zip(value.into_iter())
                .collect();
            JsValue::from_serde(&NextRecord {
                value: Some(obj),
                done: false,
            })
            .map_err(|_| JsValue::from_str("Error translating record"))
        } else {
            JsValue::from_serde(&NextRecord {
                value: None,
                done: true,
            })
            .map_err(|_| JsValue::from_str("Error translating record"))
        }
    }
}
