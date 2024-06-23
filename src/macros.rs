#[macro_export]
macro_rules! log_panic {
    ($($arg:tt)*) => {{
        log::error!($($arg)*);
        core::panic!($($arg)*)
    }}
}

#[macro_export]
macro_rules! log_and_err {
    ($($arg:tt)*) => {{
        log::error!($($arg)*);
        Err($crate::fmt::format($crate::__export::format_args!($($arg)*)))
    }}
}
