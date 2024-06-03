use std::ffi::CStr;
use std::os::raw::c_char;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};

pub use compile::*;
pub use error::*;
pub use render::*;

mod compile;
mod error;
mod render;

extern "C" {
    fn Parse(content: GoString) -> *const c_char;
}

const ERROR_PREFIX: &str = "err:";

#[repr(C)]
struct GoString {
    a: *const c_char,
    b: i64,
}

#[allow(dead_code)]
pub fn parse(content: &str) -> Result<String, D2Error> {
    unwrap_result(unsafe { Parse(content.into()) })
}

fn unwrap_result(result: *const c_char) -> Result<String, D2Error> {
    let c_str = unsafe { CStr::from_ptr(result) };
    let result = c_str
        .to_str()
        .with_context(|| "Failed to convert c_str to UTF-8")?;

    if !result.starts_with(ERROR_PREFIX) {
        Ok(result.to_string())
    } else {
        Err(D2Error::from_error_string(&result[ERROR_PREFIX.len()..]))
    }
}

impl From<&str> for GoString {
    fn from(s: &str) -> Self {
        GoString {
            a: s.as_ptr() as *const c_char,
            b: s.len() as i64,
        }
    }
}

fn null_to_default<'de, D, T>(de: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let value = Option::<T>::deserialize(de)?;
    Ok(value.unwrap_or_default())
}
