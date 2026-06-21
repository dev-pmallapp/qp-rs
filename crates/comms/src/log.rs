//! Internal debug-logging macros.
//!
//! Under the `std` feature these forward to `std`'s `print!`/`println!`/
//! `eprintln!`. On `no_std` targets they expand to no-ops (still type-checking
//! their arguments via `core::format_args!`, so format strings stay valid and
//! the referenced bindings are not flagged as unused).

#[cfg(feature = "std")]
macro_rules! cprint {
    ($($arg:tt)*) => { ::std::print!($($arg)*) };
}

#[cfg(not(feature = "std"))]
macro_rules! cprint {
    () => {{}};
    ($($arg:tt)*) => {{ let _ = ::core::format_args!($($arg)*); }};
}

#[cfg(feature = "std")]
macro_rules! cprintln {
    ($($arg:tt)*) => { ::std::println!($($arg)*) };
}

#[cfg(not(feature = "std"))]
macro_rules! cprintln {
    () => {{}};
    ($($arg:tt)*) => {{ let _ = ::core::format_args!($($arg)*); }};
}

#[cfg(feature = "std")]
macro_rules! ceprintln {
    ($($arg:tt)*) => { ::std::eprintln!($($arg)*) };
}

#[cfg(not(feature = "std"))]
macro_rules! ceprintln {
    () => {{}};
    ($($arg:tt)*) => {{ let _ = ::core::format_args!($($arg)*); }};
}
