#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub mod ddc{
    include!(concat!(env!("OUT_DIR"), "/ddc_bindings.rs"));
}

pub mod cuwf{
    include!(concat!(env!("OUT_DIR"), "/cuwf_bindings.rs"));
}
