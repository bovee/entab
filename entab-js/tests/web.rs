//! Test suite for the Web and headless browsers.

#![cfg(target_arch = "wasm32")]

use entab::Reader;
use js_sys::{Map, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn create_reader() {
    // doesn't work for obvious reasons, but it'd be nice to test against a Uint8Array
    // let data = Uint8Array::new(&JsValue::from_str(">test\nACGT"));
    let data = b">test\nACGT";
    let mut reader =
        Reader::new(data.to_vec().into_boxed_slice(), None).expect("Error creating the reader");
    assert_eq!(reader.parser(), "fasta");
    let raw_rec = reader.next().expect("Error reading first record");
    let rec = raw_rec
        .dyn_into::<Object>()
        .expect("next() returns an object");

    let done = Reflect::get(&rec, &JsValue::from_str("done")).expect("record has done");
    assert!(done.is_falsy());

    let raw_value = Reflect::get(&rec, &JsValue::from_str("value")).expect("record has value");
    let value = raw_value.dyn_into::<Map>().expect("value is a map");
    assert_eq!(value.size(), 2);
    assert_eq!(value.get(&("id".to_string()).into()), "test");
    assert!(value.has(&("sequence".to_string()).into()));
}
