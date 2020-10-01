//! Utility functions for handling low-level libR bindings.
//!
//! To delete once this functionality moves into extendr itself.
use std::os::raw;

use entab_base::error::EtError;
use extendr_api::*;
use libR_sys::{R_PreserveObject, R_xlen_t, Rf_error, Rf_allocVector, Rf_setAttrib, SET_VECTOR_ELT, VECSXP};


pub fn vec_to_list(values: &[Robj], names: Option<&Robj>) -> Robj {
    unsafe {
        let sexp = Rf_allocVector(VECSXP, values.len() as R_xlen_t);
        R_PreserveObject(sexp);
        for (idx, v) in values.iter().enumerate() {
            SET_VECTOR_ELT(sexp, idx as isize, v.clone().get());
        }
        if let Some(n) = names {
            Rf_setAttrib(sexp, Robj::namesSymbol().get(), n.get());
        }
        Robj::Owned(sexp)
    }
}


/// Convert a Result to an Robj.
pub fn unwrap_result<T>(val: Result<T, EtError>) -> Robj where T: Into<Robj> {
	match val {
		Ok(obj) => obj.into(),
		Err(err) => {
			let msg = format!("{}", err);
			unsafe {
				Rf_error(msg.as_ptr() as *const raw::c_char);
			}
			unreachable!("Code should be unreachable after call R error");
		},
	}
}
