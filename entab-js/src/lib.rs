#![allow(clippy::unused_unit)]
mod utils;

use std::collections::BTreeMap;
use std::convert::AsRef;
use std::io::{Cursor, Read};

use entab_base::error::EtError;
use entab_base::readers::{get_reader, RecordReader};
use entab_base::record::Value;
use js_sys::{Array, Object};
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
    let res = err.to_string().into();
    // technically we could just take a &EtError, but to have a nice function signature we consume
    // the err so we should also drop it in here to make clippy happy
    drop(err);
    res
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

        let (reader, parser_used) = get_reader(stream, parser.as_deref(), None).map_err(to_js)?;
        let headers = reader.headers();
        Ok(Reader {
            parser: parser_used.to_string(),
            headers,
            reader,
        })
    }

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
                .map(AsRef::as_ref)
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

#[wasm_bindgen(inline_js = "
  export function make_reader_iter(proto) { proto[Symbol.iterator] = function () { return this; }; }
")]
extern "C" {
    fn make_reader_iter(obj: &Object);
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    // this is kind of hacky, but we create a simple object and get its prototype so we can add the
    // iterable marker onto it to allow e.g. `for (row of reader) {}`
    let reader = Reader::new(b"\n".to_vec().into_boxed_slice(), Some("csv".to_string()))?;
    make_reader_iter(&Object::get_prototype_of(&reader.into()));
    Ok(())
}
