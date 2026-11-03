//! The float functions core leaves out, from libSystem's libm.

#[link(name = "System")]
unsafe extern "C" {
    #[link_name = "sin"]
    fn c_sin(x: f64) -> f64;
    #[link_name = "cos"]
    fn c_cos(x: f64) -> f64;
    #[link_name = "tan"]
    fn c_tan(x: f64) -> f64;
    #[link_name = "sqrt"]
    fn c_sqrt(x: f64) -> f64;
    #[link_name = "ceil"]
    fn c_ceil(x: f64) -> f64;
    #[link_name = "floor"]
    fn c_floor(x: f64) -> f64;
    #[link_name = "round"]
    fn c_round(x: f64) -> f64;
}

pub fn sin(x: f64) -> f64 {
    unsafe { c_sin(x) }
}

pub fn cos(x: f64) -> f64 {
    unsafe { c_cos(x) }
}

pub fn tan(x: f64) -> f64 {
    unsafe { c_tan(x) }
}

pub fn sqrt(x: f64) -> f64 {
    unsafe { c_sqrt(x) }
}

pub fn ceil(x: f64) -> f64 {
    unsafe { c_ceil(x) }
}

pub fn floor(x: f64) -> f64 {
    unsafe { c_floor(x) }
}

pub fn round(x: f64) -> f64 {
    unsafe { c_round(x) }
}
