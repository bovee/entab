use std::collections::HashMap;
use std::io::{Error, Read, Write};

use crate::BUFFER_SIZE;


struct FastaReader<'a> {
    reader: Box<dyn Read + 'a>,
}

impl<'a> Iterator for FastaReader<'a> {
    type Item = [String, String];


}
