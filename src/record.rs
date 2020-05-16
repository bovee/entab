use std::io::Write;

use crate::buffer::ReadBuffer;
use crate::EtError;

pub trait Record {
    fn size(&self) -> usize;
    fn write_field(&self, index: usize, writer: &mut dyn Write) -> Result<(), EtError>;
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
