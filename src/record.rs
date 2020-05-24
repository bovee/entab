use alloc::boxed::Box;
use alloc::vec::Vec;

use serde::Serialize;

use crate::buffer::ReadBuffer;
use crate::EtError;

pub trait Record: Serialize {
    fn size(&self) -> usize;
    fn write_field<W>(&self, index: usize, writer: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>;
    // fn get(&self, field: &str) -> Option<Value>;
}

pub trait BindT<'b> {
    type Assoc: Record;
}

pub trait ReaderBuilder: Default {
    type Item: for<'a> BindT<'a>;

    fn to_reader<'r>(
        &self,
        rb: ReadBuffer<'r>,
    ) -> Result<Box<dyn RecordReader<Item = Self::Item> + 'r>, EtError>;
}

pub trait RecordReader {
    type Item: for<'a> BindT<'a>;

    fn headers(&self) -> Vec<&str>;
    fn next(&mut self) -> Result<Option<<Self::Item as BindT>::Assoc>, EtError>;
}
