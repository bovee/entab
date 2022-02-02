//! Utility functions for handling low-level libR bindings.
//!
//! To delete once this functionality moves into extendr itself.
use core::fmt::Display;
use core::result::Result as StdResult;
use std::os::raw;

use extendr_api::prelude::*;
use libR_sys::{R_NamesSymbol, R_PreserveObject, R_xlen_t, Rf_error, Rf_allocVector, Rf_setAttrib, SET_VECTOR_ELT, VECSXP};


pub fn vec_to_list(values: &[Robj], names: Option<&Robj>) -> Robj {
    unsafe {
        let sexp = Rf_allocVector(VECSXP, values.len() as R_xlen_t);
        R_PreserveObject(sexp);
        for (idx, v) in values.iter().enumerate() {
            SET_VECTOR_ELT(sexp, idx as isize, v.clone().get());
        }
        if let Some(n) = names {
            Rf_setAttrib(sexp, R_NamesSymbol, n.get());
        }
        new_owned(sexp)
    }
}

pub fn vec_to_frame(values: &[Robj], names: &Robj) -> Robj {
    let obj = unsafe {
        let sexp = Rf_allocVector(VECSXP, values.len() as R_xlen_t);
        R_PreserveObject(sexp);
        for (idx, v) in values.iter().enumerate() {
            SET_VECTOR_ELT(sexp, idx as isize, v.clone().get());
        }
        new_owned(sexp)
    };
    unwrap_result(obj.set_attrib(names_symbol(), names));
    unwrap_result(obj.set_attrib(row_names_symbol(), (1u64..=values[0].len() as u64).collect_robj()));
    unwrap_result(obj.set_class(&["data.frame"]))
}

/// Convert a Result to an Robj.
pub fn unwrap_result(val: StdResult<Robj, impl Display>) -> Robj {
	match val {
		Ok(obj) => obj,
		Err(err) => {
			let msg = format!("{}", err);
			unsafe {
				Rf_error(msg.as_ptr() as *const raw::c_char);
			}
			unreachable!("Code should be unreachable after call R error");
		},
	}
}
